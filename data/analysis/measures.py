"""Measures and tests shared by every panel.

    diversity           mean pairwise cosine distance among one objective's hypotheses
                        (defined for objectives with at least `min_n` = 2 hypotheses)
    literature_novelty  per hypothesis, mean cosine distance to the objective's ten
                        retrieved abstracts ("literature distance" in the paper)
    soundness           per hypothesis, mean of grounding, sharpness and feasibility,
                        averaged over its three judges
    to_goal             per-hypothesis values -> per-objective mean
    paired              two-sided paired Wilcoxon signed-rank test, knockout vs baseline
    holm                Holm-Bonferroni adjustment

In the data, "baseline" is the Ideation Core / full pipeline.
"""

from __future__ import annotations

from collections import defaultdict
from pathlib import Path
import json

import numpy as np
from scipy import stats

AXES = ("grounding", "sharpness", "feasibility")


def load_jsonl(path: Path):
    return [json.loads(line) for line in path.open() if line.strip()]


def _unit(e):
    e = np.asarray(e, float)
    return e / np.clip(np.linalg.norm(e, axis=-1, keepdims=True), 1e-12, None)


def diversity(data: Path, proposals: str, min_n: int = 2):
    """Mean pairwise cosine distance within a goal. Inherently goal-level;
    undefined below two proposals, which is the only reason a goal is dropped."""
    cells = defaultdict(list)
    for r in load_jsonl(data / proposals):
        if r["success"] and r["embedding"]:
            cells[(r["label"], r["goal"])].append(r["embedding"])
    out = defaultdict(dict)
    for (c, g), embs in cells.items():
        if len(embs) < max(2, min_n):
            continue
        e = _unit(np.asarray(embs, float))
        s = e @ e.T
        iu = np.triu_indices(len(e), 1)
        out[c][g] = float((1 - s[iu]).mean())
    return out


def literature_novelty(data: Path, proposals: str, keep=None):
    """Per-proposal mean cosine distance to the goal's retrieved papers, then
    averaged within the goal. `keep`, when given, is a set of
    (label, goal, task_name) to restrict to — used to hold the proposal set fixed
    while varying which pipeline stage produced the text."""
    lit = defaultdict(list)
    for r in load_jsonl(data / "literature.jsonl"):
        lit[r["goal"]].append(r["embedding"])
    lit = {g: _unit(np.asarray(v, float)) for g, v in lit.items()}
    per_goal = defaultdict(lambda: defaultdict(list))
    for r in load_jsonl(data / proposals):
        if not (r["success"] and r["embedding"]) or r["goal"] not in lit:
            continue
        if keep is not None and (r["label"], r["goal"], r["task_name"]) not in keep:
            continue
        v = _unit(np.asarray(r["embedding"], float))
        per_goal[r["label"]][r["goal"]].append(float((1 - lit[r["goal"]] @ v).mean()))
    return per_goal


def soundness(data: Path, scores: str):
    """Overall soundness = mean of the three axes, averaged over the proposal's
    judges, then averaged within the goal."""
    by_prop = defaultdict(lambda: defaultdict(list))
    for r in load_jsonl(data / scores):
        for ax in AXES:
            by_prop[(r["config"], r["goal"], r["task"])][ax].append(r[ax])
    per_goal = defaultdict(lambda: defaultdict(list))
    n_prop = defaultdict(int)
    for (c, g, _t), axmap in by_prop.items():
        if not all(axmap[a] for a in AXES):
            continue
        per_goal[c][g].append(float(np.mean([np.mean(axmap[a]) for a in AXES])))
        n_prop[c] += 1
    return per_goal, n_prop


def to_goal(per_goal, how="mean"):
    fn = np.mean if how == "mean" else np.max
    return {c: {g: float(fn(v)) for g, v in gv.items()} for c, gv in per_goal.items()}


def paired(goalvals, ko):
    """Two-sided paired Wilcoxon of a knockout against baseline on shared goals."""
    base = goalvals.get("baseline", {})
    shared = sorted(set(base) & set(goalvals.get(ko, {})))
    d = np.array([goalvals[ko][g] - base[g] for g in shared], float)
    if len(shared) < 3 or np.allclose(d, 0):
        return len(shared), float("nan"), 1.0
    p = float(stats.wilcoxon(d, alternative="two-sided").pvalue)
    return len(shared), float(np.median(d)), p


def holm(pvals):
    """Holm-Bonferroni across the three targeted tests. Family-wise, because the
    ablation asks whether *any* stage contributes."""
    order = sorted(range(len(pvals)), key=lambda i: pvals[i])
    out, running = [0.0] * len(pvals), 0.0
    for rank, i in enumerate(order):
        running = max(running, min(1.0, pvals[i] * (len(pvals) - rank)))
        out[i] = running
    return out


