"""Panel g: the effect of each bypassed stage on each measure, on final hypotheses.

Each cell is the median per-objective difference (full pipeline minus bypass) divided
by the standard deviation of the full pipeline's per-objective values on that measure,
with a two-sided paired Wilcoxon signed-rank test on objectives both arms define.
Outlined cells are the pre-specified tests of panels d, e (after Refinement) and f,
Holm-adjusted across those three; the other six cells are Holm-adjusted among
themselves.
"""

from __future__ import annotations

import numpy as np
import matplotlib.pyplot as plt
from matplotlib.colors import LinearSegmentedColormap
from scipy import stats

from measures import diversity, holm, literature_novelty, soundness, to_goal

CORE = "baseline"   # the full pipeline
ARMS = ["baseline", "embody_bypass", "elements_bypass", "debate_bypass"]
NAME = {"baseline": "full pipeline", "embody_bypass": "Formulation bypassed",
        "elements_bypass": "Element Selection bypassed",
        "debate_bypass": "Refinement bypassed"}
SCORES = "soundness_scores_hypos.jsonl"   # raw-hypothesis soundness, all four arms
INK = "#1f1f1e"
# diverging: red (stage lowers the measure) <-> neutral gray <-> blue (stage raises it)
DIVERGE = LinearSegmentedColormap.from_list("rb", ["#e34948", "#f0efec", "#2a78d6"])
MEASURES = ["inter-hypothesis diversity", "literature distance", "scientific soundness"]


def measures(D):
    """Per-objective values for every ablation arm on the three measures, raw hypotheses."""
    div = diversity(D, "hypotheses.jsonl", 2)
    nov = to_goal(literature_novelty(D, "hypotheses.jsonl"))
    snd = to_goal(soundness(D, SCORES)[0])
    return [div, nov, snd]


def stage_effects(D):
    """(E, P, N, adj): standardized effects, raw P, n objectives and adjusted P per cell
    (row = bypassed stage, column = measure)."""
    meas = measures(D)
    kos = ARMS[1:]
    pre = {("embody_bypass", 0), ("elements_bypass", 1), ("debate_bypass", 2)}
    E = np.full((len(kos), len(meas)), np.nan)
    P, N = {}, {}
    for i, ko in enumerate(kos):
        for j, gv in enumerate(meas):
            shared = sorted(set(gv[CORE]) & set(gv[ko]))
            b = np.array([gv[CORE][g] for g in shared]); k = np.array([gv[ko][g] for g in shared])
            sd = np.array(list(gv[CORE].values()), float).std(ddof=1)
            E[i, j] = np.median(b - k) / sd
            P[(i, j)] = stats.wilcoxon(b, k).pvalue if np.any(b != k) else 1.0
            N[(i, j)] = len(shared)
    pre_keys = [(i, j) for (i, j) in P if (kos[i], j) in pre]
    off_keys = [k for k in P if k not in pre_keys]
    adj = dict(zip(pre_keys, holm([P[k] for k in pre_keys])))
    adj.update(zip(off_keys, holm([P[k] for k in off_keys])))
    return E, P, N, adj, pre_keys


def draw(ax, E, adj, pre_keys, title):
    kos = ARMS[1:]
    vmax = max(1.0, float(np.nanmax(np.abs(E))))
    im = ax.imshow(E, cmap=DIVERGE, vmin=-vmax, vmax=vmax, aspect="auto")
    for (i, j), q in adj.items():
        star = "***" if q < 0.001 else "**" if q < 0.01 else "*" if q < 0.05 else ""
        ax.text(j, i, f"{E[i, j]:+.2f}{star}", ha="center", va="center", fontsize=9,
                color="white" if abs(E[i, j]) > 0.6 * vmax else INK)
        if (i, j) in pre_keys:
            ax.add_patch(plt.Rectangle((j - 0.47, i - 0.47), 0.94, 0.94, fill=False,
                                       edgecolor=INK, lw=1.6))
    ax.set_xticks(range(E.shape[1]))
    ax.set_yticks(range(len(kos)))
    ax.set_xticklabels(["diversity", "literature\ndistance", "soundness"], fontsize=8.5)
    ax.set_yticklabels(["Formulation", "Element\nSelection", "Refinement"], fontsize=9)
    ax.set_xticks(np.arange(-0.5, E.shape[1]), minor=True)
    ax.set_yticks(np.arange(-0.5, len(kos)), minor=True)
    ax.grid(which="minor", color="white", lw=2)
    ax.tick_params(which="both", length=0)
    for s in ax.spines.values():
        s.set_visible(False)
    ax.set_title(title, fontsize=11, color=INK)
    ax.set_xlabel("measure (final hypotheses)", fontsize=9, color=INK)
    ax.set_ylabel("stage", fontsize=9, color=INK)
    cb = ax.figure.colorbar(im, ax=ax, fraction=0.05, pad=0.03)
    cb.set_label("effect of the stage\n(full − bypass, SD units)", fontsize=8, color=INK)
    cb.ax.tick_params(labelsize=7.5)
    cb.outline.set_visible(False)
