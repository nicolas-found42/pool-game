"""Shared scoring network + SB3 policy for the pool shot-selection prototype.

Architecture (frozen: it is mirrored by the Rust/ORT serving path, ticket #16)
---------------------------------------------------------------------------
  ctx        = MLP_obs(obs)                    # 64 -> 64 -> 64   (per shot)
  c_k        = MLP_cand(cand_k)                # 16 -> 64 -> 64   (shared over candidates)
  raw_logit_k= MLP_combine([ctx, c_k])         # 128 -> 64 -> 1
  value      = MLP_value([ctx, mean_k c_k])    # 128 -> 64 -> 1   (NOT exported to ONNX)

``ScoringNet.forward(obs, cand)`` returns the RAW logits of shape ``[B, K]``.
Invalid-action masking is deliberately *outside* the graph: the Rust side
applies the mask itself (see ``environment.py`` for the mask semantics:
1/True = legal).

Every layer is exactly the widths above; ``HIDDEN = 64`` and the 64/16 dims are
part of the cross-language contract, not tunables.
"""

from __future__ import annotations

import numpy as np
import torch as th
from sb3_contrib.common.maskable.policies import MaskableActorCriticPolicy
from stable_baselines3.common.torch_layers import BaseFeaturesExtractor
from torch import nn

OBS_DIM = 64
CAND_DIM = 16
HIDDEN = 64


class ScoringNet(nn.Module):
    """Per-candidate shared scoring head. Exported verbatim to ONNX (minus value head)."""

    def __init__(self, obs_dim: int = OBS_DIM, cand_dim: int = CAND_DIM, hidden: int = HIDDEN) -> None:
        super().__init__()
        self.obs_mlp = nn.Sequential(nn.Linear(obs_dim, hidden), nn.ReLU(), nn.Linear(hidden, hidden), nn.ReLU())
        self.cand_mlp = nn.Sequential(nn.Linear(cand_dim, hidden), nn.ReLU(), nn.Linear(hidden, hidden), nn.ReLU())
        self.combine = nn.Sequential(nn.Linear(2 * hidden, hidden), nn.ReLU(), nn.Linear(hidden, 1))
        self.value_mlp = nn.Sequential(nn.Linear(2 * hidden, hidden), nn.ReLU(), nn.Linear(hidden, 1))

    # -- pieces (named so the ONNX tracer sees plain module calls)
    def ctx(self, obs: th.Tensor) -> th.Tensor:
        return self.obs_mlp(obs)

    def cand_emb(self, cand: th.Tensor) -> th.Tensor:
        """[B, K, 16] -> [B, K, 64]."""
        return self.cand_mlp(cand)

    def forward(self, obs: th.Tensor, cand: th.Tensor) -> th.Tensor:
        """Raw logits [B, K] (no masking applied here)."""
        c = self.cand_emb(cand)
        ctx = self.ctx(obs).unsqueeze(1).expand(-1, c.shape[1], -1)
        return self.combine(th.cat([ctx, c], dim=-1)).squeeze(-1)

    def value(self, obs: th.Tensor, cand: th.Tensor) -> th.Tensor:
        """State value [B] from ctx + mean candidate embedding."""
        return self.value_mlp(th.cat([self.ctx(obs), self.cand_emb(cand).mean(dim=1)], dim=-1)).squeeze(-1)

    def n_params(self) -> int:
        return sum(p.numel() for p in self.parameters())


class ObsVectorExtractor(BaseFeaturesExtractor):
    """Identity extractor: the encoder work lives in ScoringNet, not in SB3's feature stack.

    It exists only to satisfy SB3's requirement of a custom feature extractor for
    ``spaces.Dict`` observations; ``features_dim`` is the obs branch input width.
    """

    def __init__(self, observation_space, features_dim: int = OBS_DIM) -> None:
        super().__init__(observation_space, features_dim)

    def forward(self, observations):  # type: ignore[override]
        return observations["obs"]


class CandidateScoringPolicy(MaskableActorCriticPolicy):
    """MaskablePPO policy with the shared per-candidate scorer as its action head.

    SB3's ``mlp_extractor``/``action_net``/``value_net`` stack cannot express a
    score that depends on a variable-length candidate axis, so ``_build`` is
    replaced by ``ScoringNet`` and the four methods that SB3's rollout/train loop
    calls are overridden. Everything else (optimizer, masking, rollouts, GAE) is
    stock sb3-contrib behaviour.
    """

    def __init__(self, observation_space, action_space, lr_schedule, **kwargs) -> None:
        kwargs.setdefault("features_extractor_class", ObsVectorExtractor)
        kwargs.setdefault("features_extractor_kwargs", {"features_dim": OBS_DIM})
        kwargs.setdefault("net_arch", [])
        super().__init__(observation_space, action_space, lr_schedule, **kwargs)

    # -- construction ------------------------------------------------------
    def _build(self, lr_schedule) -> None:
        self.mlp_extractor = nn.Identity()
        self.action_net = nn.Identity()
        self.value_net = nn.Identity()
        self.scorer = ScoringNet(OBS_DIM, CAND_DIM, HIDDEN)
        self._init_scorer_weights()
        self.optimizer = self.optimizer_class(self.parameters(), lr=lr_schedule(1), **self.optimizer_kwargs)  # type: ignore[call-arg]

    def _init_scorer_weights(self) -> None:
        """Orthogonal init, gain sqrt(2) hidden / 0.01 on the two decision heads."""
        heads = {self.scorer.combine[-1], self.scorer.value_mlp[-1]}
        for module in self.scorer.modules():
            if isinstance(module, nn.Linear):
                gain = 0.01 if module in heads else np.sqrt(2)
                nn.init.orthogonal_(module.weight, gain=gain)
                nn.init.zeros_(module.bias)

    # -- helpers -----------------------------------------------------------
    @staticmethod
    def _mask(obs) -> th.Tensor:
        return obs["mask"].bool()

    def _logits(self, obs) -> th.Tensor:
        return self.scorer(obs["obs"], obs["cand"])

    def _dist(self, obs, action_masks=None):
        distribution = self.action_dist.proba_distribution(action_logits=self._logits(obs))
        masks = self._mask(obs) if action_masks is None else action_masks
        if masks is not None:
            distribution.apply_masking(masks)
        return distribution

    # -- overridden SB3/SB3-contrib hooks ---------------------------------
    def get_distribution(self, obs, action_masks=None):
        return self._dist(obs, action_masks)

    def forward(self, obs, deterministic: bool = False, action_masks=None):
        distribution = self._dist(obs, action_masks)
        actions = distribution.get_actions(deterministic=deterministic)
        log_prob = distribution.log_prob(actions)
        return actions, self.predict_values(obs), log_prob

    def evaluate_actions(self, obs, actions, action_masks=None):
        distribution = self._dist(obs, action_masks)
        log_prob = distribution.log_prob(actions)
        return self.predict_values(obs), log_prob, distribution.entropy()

    def predict_values(self, obs) -> th.Tensor:
        return self.scorer.value(obs["obs"], obs["cand"])

    def _predict(self, observation, deterministic: bool = False, action_masks=None) -> th.Tensor:
        return self._dist(observation, action_masks).get_actions(deterministic=deterministic)
