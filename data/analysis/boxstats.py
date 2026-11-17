"""Box-plot drawing and colours shared by the figure panels."""
from __future__ import annotations

import numpy as np
from scipy import stats

PIPE_ABLATIONS = ["embody_bypass", "elements_bypass", "debate_bypass"]
SINGLE = ["gemini_31_pro_preview", "gpt_52_high", "claude_opus_46", "grok_41_fast_reasoning"]
PIPE = ["baseline"] + PIPE_ABLATIONS
CONFIGS = PIPE + SINGLE

COMPARISON = ["baseline"] + SINGLE

LABEL = {
    "baseline": "baseline\n(full pipeline)", "embody_bypass": "formulation\nbypass",
    "elements_bypass": "element\nselection\nbypass", "debate_bypass": "refinement\nbypass",
    "gemini_31_pro_preview": "Gemini 3.1\nPro", "gpt_52_high": "GPT-5.2\nHigh",
    "claude_opus_46": "Opus 4.6", "grok_41_fast_reasoning": "Grok 4.1\nFast",
}
# Two names for the same arm, because it plays two roles.
# Comparison frame: the system is being weighed against whole external models, so
# it is named for what was actually run — the Ideation Core, whose output the
# writing stage only reformats.
# Ablation frame: the other arms are single-stage knockouts of this same system,
# so the intact pipeline is their control. "full pipeline" is the conventional
# ablation label; "wild type" was tried and dropped — it is the genetics idiom, not
# the systems one, and the research goals here are biomedical, so it invites being
# read as biological material rather than a software configuration.
LABEL_COMPARISON = {**LABEL, "baseline": "Ideation Core"}
LABEL_ABLATION = {**LABEL, "baseline": "full pipeline"}

COLOR = {
    "baseline": "#1B4F9B", "embody_bypass": "#04845F",
    "elements_bypass": "#5B1EA6", "debate_bypass": "#B3004D",
    "gemini_31_pro_preview": "#C04000", "gpt_52_high": "#B8860B",
    "claude_opus_46": "#5A7F19", "grok_41_fast_reasoning": "#4A4E54",
}
PIPE_MARKER = "o"
SINGLE_MARKER = "^"

def marker_of(c):
    return PIPE_MARKER if c in PIPE else SINGLE_MARKER


def _fmt_p(q):
    if not np.isfinite(q):
        return "—"
    return "P<0.001" if q < 1e-3 else f"P={q:.3f}"


def _ci95(v):
    """(mean, half-width of 95% CI) for a 1-D sample."""
    v = np.asarray(v, float)
    n = len(v)
    if n < 2:
        return (float(v.mean()) if n else np.nan), 0.0
    sem = v.std(ddof=1) / np.sqrt(n)
    return float(v.mean()), float(sem * stats.t.ppf(0.975, n - 1))


def panel(ax, goalvals: dict, stats_by_config: dict, ylabel: str, title: str,
          configs=CONFIGS, ylim=None, kind="box", rng=None, labels=None,
          show_p=False, grid=True, separator=True):
    rng = rng or np.random.default_rng(0)
    labels = labels or LABEL
    data, colors, ns = [], [], []
    for c in configs:
        vals = list(goalvals.get(c, {}).values())
        data.append(np.asarray(vals, float))
        colors.append(COLOR[c])
        ns.append(len(vals))
    pos = np.arange(len(configs))
    box_tops = []

    if kind == "box":
        bp = ax.boxplot(
            [d if len(d) else [np.nan] for d in data], positions=pos, widths=0.62,
            whis=1.5, showfliers=False, patch_artist=True,
            medianprops=dict(color="#222222", lw=1.6),
            whiskerprops=dict(color="#666666", lw=1.0), capprops=dict(color="#666666", lw=1.0),
            boxprops=dict(lw=1.0, edgecolor="#666666"), zorder=2)
        for patch, col in zip(bp["boxes"], colors):
            patch.set_facecolor(col)
            patch.set_alpha(0.35)
        for vals in data:
            if not len(vals):
                box_tops.append(0)
                continue
            q1, q3 = np.percentile(vals, [25, 75])
            fence = vals[vals <= q3 + 1.5 * (q3 - q1)]
            box_tops.append(float(fence.max()) if len(fence) else float(vals.max()))
    else:  # dot + 95% CI
        for i, (vals, col) in enumerate(zip(data, colors)):
            if not len(vals):
                box_tops.append(0)
                continue
            m, h = _ci95(vals)
            mk = marker_of(configs[i])
            ax.errorbar(i, m, yerr=h, fmt=mk, color=col, ms=7, capsize=4,
                        elinewidth=1.4, capthick=1.4, zorder=3)
            box_tops.append(m + h)

    if separator:
        has_both = any(c in PIPE for c in configs) and any(c in SINGLE for c in configs)
        if has_both:
            pipe_count = sum(1 for c in configs if c in PIPE)
            ax.axvline(pipe_count - 0.5, color="grey", lw=0.8, ls=":")
    for i, c in enumerate(configs):
        if c == "baseline":
            continue
        _, _, q = stats_by_config.get(c, (0, 0, float("nan")))
        if not np.isfinite(q):
            continue
        if show_p:
            star = "*" if q < 0.05 else ""
            txt = _fmt_p(q) + star
        else:
            if q < 0.001:
                txt = "***"
            elif q < 0.01:
                txt = "**"
            elif q < 0.05:
                txt = "*"
            else:
                txt = "n.s."
        ax.annotate(txt, (i, box_tops[i]), xytext=(0, 4),
                    textcoords="offset points", ha="center", va="bottom",
                    fontsize=9, fontweight="bold" if not show_p else "normal",
                    color="#111111" if (np.isfinite(q) and q < 0.05) else "#888888")
    ax.set_xticks(pos)
    ax.set_xticklabels([f"{labels[c]}\nn={ns[i]}" for i, c in enumerate(configs)], fontsize=9)
    ax.set_ylabel(ylabel, fontsize=12)
    ax.set_title(title, fontsize=13, pad=8)
    ax.tick_params(axis="y", labelsize=9)
    if ylim:
        ax.set_ylim(ylim[0], ylim[1] + 0.6)
    if grid:
        ax.grid(axis="y", lw=0.3, color="#cccccc")
        ax.set_axisbelow(True)
    for s in ("top", "right"):
        ax.spines[s].set_visible(False)
