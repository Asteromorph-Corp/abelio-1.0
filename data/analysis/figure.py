#!/usr/bin/env python3
"""Fig. 2: the Abelio Ideation Core compared with frontier language models (a-c) and
its single-stage ablation (d-g).

    a  literature distance vs soundness, every hypothesis (descriptive)
    b  fraction of hypotheses above a joint bar on both measures (descriptive)
    c  inter-hypothesis diversity, Ideation Core vs each frontier model
    d  Formulation bypassed            -> inter-hypothesis diversity
    e  Element Selection bypassed      -> literature distance, before and after Refinement
    f  Refinement bypassed             -> scientific soundness
    g  each bypassed stage on all three measures

a-c score the writing-stage summary of each Ideation Core hypothesis (matched to the
frontier models' output format); d-g score hypotheses as generated. In the data,
"baseline" is the Ideation Core / full pipeline.

    uv run python analysis/figure.py --data data --out results
"""

from __future__ import annotations

import argparse
import sys
from collections import defaultdict
from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np
from scipy.stats import gaussian_kde

sys.path.insert(0, str(Path(__file__).resolve().parent))
import boxstats as bs  # noqa: E402
import stage_effects  # noqa: E402
from measures import (  # noqa: E402
    diversity,
    holm,
    literature_novelty,
    load_jsonl,
    paired,
    soundness,
    to_goal,
)

FS_AXIS, FS_TITLE, FS_TICK, FS_LEGEND = 11, 12, 9, 8.5
LBL_ABL = bs.LABEL_ABLATION


def _unit(e):
    e = np.asarray(e, float)
    return e / np.clip(np.linalg.norm(e, axis=-1, keepdims=True), 1e-12, None)


def per_proposal(data: Path, proposals: str, single: str, scores: str):
    """Novelty and soundness keyed by proposal, for the density panel. The panel is
    descriptive — every inferential claim in the caption is made on goal means."""
    lit = defaultdict(list)
    for r in load_jsonl(data / "literature.jsonl"):
        lit[r["goal"]].append(r["embedding"])
    lit = {g: _unit(np.asarray(v, float)) for g, v in lit.items()}

    nov = {c: {} for c in bs.CONFIGS}
    for name in (proposals, single):
        p = data / name
        if not p.is_file():
            continue
        for r in load_jsonl(p):
            if not (r["success"] and r["embedding"]) or r["goal"] not in lit:
                continue
            if r["label"] not in nov:
                continue
            v = _unit(np.asarray(r["embedding"], float))
            nov[r["label"]][f"{r['goal']}/{r['task_name']}"] = float(
                (1 - lit[r["goal"]] @ v).mean())

    axes_ = ("grounding", "sharpness", "feasibility")
    by_prop = defaultdict(lambda: defaultdict(list))
    for r in load_jsonl(data / scores):
        for ax in axes_:
            by_prop[(r["config"], r["goal"], r["task"])][ax].append(r[ax])
    snd = {c: {} for c in bs.CONFIGS}
    for (c, g, t), axmap in by_prop.items():
        if c in snd and all(axmap[a] for a in axes_):
            snd[c][f"{g}/{t}"] = float(np.mean([np.mean(axmap[a]) for a in axes_]))
    return nov, snd


def goal_means(per_prop_map):
    """proposal-keyed 'goal/task' -> per-goal mean, the unit every test uses."""
    out = defaultdict(lambda: defaultdict(list))
    for c, kv in per_prop_map.items():
        for key, v in kv.items():
            out[c][key.split("/", 1)[0]].append(v)
    return {c: {g: float(np.mean(v)) for g, v in gv.items()} for c, gv in out.items()}


def draw_scatter(ax, nov, snd, configs, labels):
    """Density of proposals in the novelty-soundness plane, one contour set per arm."""
    allv = lambda m: np.array([v for c in configs for v in m[c].values()], float)  # noqa: E731
    zx_mu, zx_sd = allv(nov).mean(), allv(nov).std(ddof=1)
    zy_mu, zy_sd = allv(snd).mean(), allv(snd).std(ddof=1)
    xlim, ylim = (-2.5, 4.7), (-3.0, 3.0)
    xx, yy = np.meshgrid(np.linspace(*xlim, 150), np.linspace(*ylim, 150))
    grid = np.vstack([xx.ravel(), yy.ravel()])

    for c in configs:
        keys = sorted(set(nov[c]) & set(snd[c]))
        if len(keys) < 10:
            continue
        x = (np.array([nov[c][k] for k in keys]) - zx_mu) / zx_sd
        y = (np.array([snd[c][k] for k in keys]) - zy_mu) / zy_sd
        color = bs.COLOR[c]
        kde = gaussian_kde(np.vstack([x, y]))
        zz = kde(grid).reshape(xx.shape)
        dens = np.sort(kde(np.vstack([x, y])))
        lvl90, lvl50 = dens[int(0.10 * len(dens))], dens[int(0.50 * len(dens))]
        ax.contourf(xx, yy, zz, levels=[lvl50, zz.max() * 2], colors=[color], alpha=0.20)
        ax.contour(xx, yy, zz, levels=[lvl50], colors=[color], linewidths=[1.0], alpha=0.6)
        ax.contour(xx, yy, zz, levels=[lvl90], colors=[color], linewidths=[0.6],
                   linestyles="dashed", alpha=0.4)
        mk = bs.marker_of(c)
        ax.plot(x.mean(), y.mean(), mk, color=color, ms=8, zorder=4,
                markeredgecolor="white", markeredgewidth=0.8)
        ax.scatter([], [], marker=mk, color=color, s=50,
                   label=f"{labels[c].replace(chr(10), ' ')} (n={len(keys)})")

    ax.set_xlim(xlim)
    ax.set_ylim(ylim)
    ax.legend(fontsize=FS_LEGEND, loc="lower right", handletextpad=0.4, borderpad=0.4)
    ax.set_xlabel("literature distance  (z, → further from prior work)", fontsize=FS_AXIS)
    ax.set_ylabel("scientific soundness  (z, ↑ better)", fontsize=FS_AXIS)
    ax.set_title("Literature distance vs soundness", fontsize=FS_TITLE, pad=6)
    ax.tick_params(labelsize=FS_TICK)
    for s in ("top", "right"):
        ax.spines[s].set_visible(False)


def draw_sweep(ax, nov, snd, configs, labels, n_boot=1000, seed=0, legend=True):
    """Fraction of each arm's hypotheses above-right of the line y = -x + b in panel
    a's plane, as b sweeps. Same per-hypothesis values and pooled z-scores as panel a;
    bands are 95% intervals from resampling objectives. Returns (bgrid, {arm: curve})."""
    rng = np.random.default_rng(seed)
    keys = {c: sorted(set(nov[c]) & set(snd[c])) for c in configs}
    allx = np.array([nov[c][k] for c in configs for k in keys[c]])
    ally = np.array([snd[c][k] for c in configs for k in keys[c]])
    mx, sx, my, sy = allx.mean(), allx.std(ddof=1), ally.mean(), ally.std(ddof=1)
    zs = {c: {k: (nov[c][k] - mx) / sx + (snd[c][k] - my) / sy for k in keys[c]}
          for c in configs}
    pooled = np.concatenate([list(zs[c].values()) for c in configs])
    bgrid = np.linspace(np.percentile(pooled, 0.5), np.percentile(pooled, 99.5), 160)

    def share(vals):
        v = np.sort(vals)
        return 1 - np.searchsorted(v, bgrid, side="left") / len(v)

    curves = {}
    for c in configs:
        by_goal = defaultdict(list)
        for k, v in zs[c].items():
            by_goal[k.split("/", 1)[0]].append(v)
        goals = sorted(by_goal)
        curve = share(np.array(list(zs[c].values())))
        boots = np.empty((n_boot, len(bgrid)))
        for i in range(n_boot):
            pick = rng.integers(0, len(goals), len(goals))
            boots[i] = share(np.concatenate([by_goal[goals[j]] for j in pick]))
        lo, hi = np.percentile(boots, [2.5, 97.5], axis=0)
        ax.fill_between(bgrid, lo, hi, color=bs.COLOR[c], alpha=0.14, lw=0)
        ax.plot(bgrid, curve, color=bs.COLOR[c], lw=2.0 if c == "baseline" else 1.4,
                label=labels[c].replace(chr(10), " "))
        curves[c] = curve
    ax.set_ylim(0, 1.02)
    ax.yaxis.set_major_formatter(plt.FuncFormatter(lambda v, _: f"{v:.0%}"))
    ax.set_xlabel("joint bar b  (z distance + z soundness ≥ b)", fontsize=FS_AXIS)
    ax.set_ylabel("fraction of hypotheses above the bar", fontsize=FS_AXIS)
    ax.set_title("Literature distance + soundness", fontsize=FS_TITLE, pad=6)
    ax.tick_params(labelsize=FS_TICK)
    ax.grid(axis="y", color="#d9d8d4", lw=0.6)
    ax.set_axisbelow(True)
    for s in ("top", "right"):
        ax.spines[s].set_visible(False)
    if legend:
        ax.legend(fontsize=FS_LEGEND, frameon=False, loc="upper right")
    return bgrid, curves


def restrict(gv, _configs):
    """Each baseline over the goals it shares with Abelio — the goals of its own
    paired test — and Abelio over all of its own. Matching every arm to a single
    common set instead would let one baseline's misses shrink another's comparison
    (Opus is undefined on three goals the others cover)."""
    base = set(gv["baseline"])
    kept = {c: (gv[c] if c == "baseline" else {g: v for g, v in gv[c].items() if g in base})
            for c in gv}
    return kept, sorted(base)


def p_label(ax, pos, vals, p, fontsize=8.5):
    """Exact P above a box, in place of a threshold asterisk.

    Nature's statistics policy asks for exact values: `*` collapses 0.049 and
    0.00001 into one glyph and hides how close a result is to the threshold. The
    anchor reproduces `bs.panel`'s whisker rule so the label clears the whisker.
    """
    v = np.asarray(vals, float)
    q1, q3 = np.percentile(v, [25, 75])
    fence = v[v <= q3 + 1.5 * (q3 - q1)]
    top = float(fence.max()) if len(fence) else float(v.max())
    txt = "$P$ < 0.001" if p < 0.001 else f"$P$ = {p:.3f}"
    ax.annotate(txt, (pos, top), xytext=(0, 5), textcoords="offset points",
                ha="center", va="bottom", fontsize=fontsize,
                color="#111111" if p < 0.05 else "#555555")


def headroom(ax, frac=0.11, pad=9):
    """Open space between the tallest whisker, the significance mark above it and
    the title. `bs.panel` leaves the axes tight to the data, so on a short panel the
    asterisk lands almost on the title — worse where the title runs to two lines."""
    lo, hi = ax.get_ylim()
    ax.set_ylim(lo, hi + frac * (hi - lo))
    ax.title.set_y(1.0)
    ax.set_title(ax.get_title(), fontsize=FS_TITLE, pad=pad)


def two_arm(ax, gv, ko, ylabel, title, q, n):
    stat = {"baseline": (len(gv.get("baseline", {})), 0.0, float("nan")),
            ko: (n, 0.0, float("nan"))}
    bs.panel(ax, gv, stat, ylabel=ylabel, title=title, configs=["baseline", ko],
             kind="box", labels=LBL_ABL, grid=False, separator=False)
    ax.set_xticklabels([f"full pipeline\nn={len(gv['baseline'])}",
                        f"{bs.LABEL[ko]}\nn={len(gv[ko])}"], fontsize=FS_TICK)
    ax.set_ylabel(ylabel, fontsize=FS_AXIS)
    ax.set_title(title, fontsize=FS_TITLE, pad=6)
    p_label(ax, 1, list(gv[ko].values()), q)
    headroom(ax)


def two_arm_grouped(ax, groups, ko, ylabel, title):
    """One stage measured at two points, as grouped pairs (full pipeline | bypass)
    with a bracketed P over each pair. `groups` is [(label, goalvals, q, n), ...]."""
    gap, half = 1.6, 0.3
    tops = []
    for gi, (label, gv, q, n) in enumerate(groups):
        x0 = gi * gap
        vals = [np.asarray(list(gv[c].values()), float) for c in ("baseline", ko)]
        bp = ax.boxplot(vals, positions=[x0 - half, x0 + half], widths=0.5, whis=1.5,
                        showfliers=False, patch_artist=True,
                        medianprops=dict(color="#222222", lw=1.6),
                        whiskerprops=dict(color="#555555"), capprops=dict(color="#555555"),
                        boxprops=dict(edgecolor="#777777"))
        for patch, c in zip(bp["boxes"], ("baseline", ko)):
            patch.set_facecolor(bs.COLOR[c])
            patch.set_alpha(0.35)
        top = max(float(w.get_ydata().max()) for w in bp["caps"])
        tops.append((x0, top, q))
    lo, hi = ax.get_ylim()
    span = hi - lo
    for x0, top, q in tops:
        y = top + 0.05 * span
        ax.plot([x0 - half, x0 - half, x0 + half, x0 + half],
                [y - 0.02 * span, y, y, y - 0.02 * span], color="#222222", lw=1.1)
        txt = "$P$ < 0.001" if q < 0.001 else f"$P$ = {q:.3f}"
        ax.text(x0, y + 0.012 * span, txt, ha="center", va="bottom", fontsize=FS_TICK,
                color="#111111" if q < 0.05 else "#555555")
    ax.set_ylim(lo, max(t for _, t, _ in tops) + 0.2 * span)
    # boxes named on the axis like the neighbouring panels; group name as a second tier
    pos = [gi * gap + dx for gi in range(len(groups)) for dx in (-half, half)]
    ax.set_xticks(pos)
    ax.set_xticklabels(["full\npipeline", "selection\nbypass"] * len(groups), fontsize=FS_TICK)
    ax.tick_params(axis="x", length=0)
    from matplotlib.transforms import blended_transform_factory
    tr = blended_transform_factory(ax.transData, ax.transAxes)
    for gi, (label, _gv, _q, n) in enumerate(groups):
        x0 = gi * gap
        ax.plot([x0 - half - 0.22, x0 + half + 0.22], [-0.16, -0.16], color="#999999", lw=0.8,
                transform=tr, clip_on=False)
        ax.text(x0, -0.19, f"{label}\nn={n}", transform=tr, ha="center", va="top",
                fontsize=FS_TICK)
    ax.set_xlim(-0.75, (len(groups) - 1) * gap + 0.75)
    ax.set_ylabel(ylabel, fontsize=FS_AXIS)
    ax.set_title(title, fontsize=FS_TITLE, pad=6)
    ax.tick_params(axis="y", labelsize=FS_TICK)
    for s in ("top", "right"):
        ax.spines[s].set_visible(False)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", type=Path, default=Path("data"))
    ap.add_argument("--out", type=Path, default=Path("results"))
    ap.add_argument("--name", default="fig2")
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    D = args.data
    min_n = 2

    # Comparison with frontier models: writing-stage summaries
    nov_pp, snd_pp = per_proposal(D, "hypotheses_summarized.jsonl",
                                  "single_model.jsonl", "soundness_scores_summarized.jsonl")
    div_abs = diversity(D, "hypotheses_summarized.jsonl", min_n)
    for c in bs.CONFIGS:                       # frontier models come from a second file
        div_abs.setdefault(c, {})
    for c, gv in diversity(D, "single_model.jsonl", min_n).items():
        div_abs[c].update(gv)
    div_cmp, common = restrict({c: div_abs[c] for c in bs.CONFIGS}, bs.COMPARISON)

    nov_goal, snd_goal = goal_means(nov_pp), goal_means(snd_pp)
    cmp_arms = [c for c in bs.COMPARISON if c != "baseline"]
    cmp_stats = {}
    for name, gv in (("diversity", div_cmp), ("literature distance", nov_goal),
                     ("soundness", snd_goal)):
        res = {a: paired(gv, a) for a in cmp_arms}
        qs = holm([res[a][2] for a in cmp_arms])                 # Holm across 4 models
        cmp_stats[name] = {a: (res[a][0], res[a][1], q) for a, q in zip(cmp_arms, qs)}

    # Ablation: hypotheses as generated
    div_idea = diversity(D, "hypotheses.jsonl", min_n)
    nov_idea = to_goal(literature_novelty(D, "hypotheses.jsonl"))
    snd_idea = to_goal(soundness(D, "soundness_scores_hypos.jsonl")[0])
    nov_pre = to_goal(literature_novelty(D, "hypotheses_predebate.jsonl"))
    ABL = [(div_idea, "embody_bypass", "inter-hypothesis diversity", "Formulation"),
           (nov_idea, "elements_bypass", "literature distance", "Element Selection"),
           (nov_pre, "elements_bypass", "literature distance",
            "Element Selection\n(Before Refinement)"),
           (snd_idea, "debate_bypass", "scientific soundness (1–10)", "Refinement")]
    ABL = [({c: {g: v for g, v in gvc.items()
                 if g in set(gv["baseline"]) & set(gv[ko])} for c, gvc in gv.items()},
            ko, yl, t) for gv, ko, yl, t in ABL]
    abl_res = [paired(gv, ko) for gv, ko, *_ in ABL]
    # Holm across the three stages (d, e after Refinement, f); e before Refinement
    # repeats the Element Selection comparison and is not adjusted.
    stage_ix = [0, 1, 3]
    abl_q = [None] * len(abl_res)
    for i, q in zip(stage_ix, holm([abl_res[i][2] for i in stage_ix])):
        abl_q[i] = q
    abl_q = [q if q is not None else abl_res[i][2] for i, q in enumerate(abl_q)]
    E, _P, _N, adj, pre_keys = stage_effects.stage_effects(D)

    fig = plt.figure(figsize=(18.0, 10.0))
    outer = fig.add_gridspec(2, 1, height_ratios=[1, 0.88], hspace=0.40)
    top = outer[0].subgridspec(1, 3, width_ratios=[1.5, 1.0, 1.1], wspace=0.30)
    bot = outer[1].subgridspec(1, 4, width_ratios=[1, 1.75, 1, 1.55], wspace=0.45)
    ax_a, ax_s, ax_b = (fig.add_subplot(top[0]), fig.add_subplot(top[1]),
                        fig.add_subplot(top[2]))
    row2 = [fig.add_subplot(bot[i]) for i in range(3)]
    ax_h = fig.add_subplot(bot[3])

    div_s = cmp_stats["diversity"]
    draw_scatter(ax_a, nov_pp, snd_pp, bs.COMPARISON, bs.LABEL_COMPARISON)
    draw_sweep(ax_s, nov_pp, snd_pp, bs.COMPARISON, bs.LABEL_COMPARISON,
                               legend=False)
    h, l = ax_a.get_legend_handles_labels()
    ax_a.get_legend().remove()
    ax_a.legend(h, l, fontsize=FS_LEGEND, loc="upper right", frameon=True,
                framealpha=0.9, edgecolor="none", facecolor="white",
                handletextpad=0.4, borderpad=0.4)
    ax_s.legend(fontsize=FS_LEGEND, frameon=False, loc="upper right", handlelength=2.0)

    st_b = {c: (0, 0.0, float("nan")) for c in bs.COMPARISON}
    bs.panel(ax_b, div_cmp, st_b, ylabel="mean pairwise cosine distance",
             title="Inter-hypothesis diversity", configs=bs.COMPARISON, kind="box",
             labels=bs.LABEL_COMPARISON, grid=False, separator=False)
    ax_b.set_ylabel("mean pairwise cosine distance", fontsize=FS_AXIS)
    ax_b.set_title("Inter-hypothesis diversity", fontsize=FS_TITLE, pad=6)
    ax_b.tick_params(axis="y", labelsize=FS_TICK)
    for i, c in enumerate(bs.COMPARISON):
        if c != "baseline":
            p_label(ax_b, i, list(div_cmp[c].values()), div_s[c][2], fontsize=8)
    headroom(ax_b, frac=0.14)
    ax_b.set_xticklabels([lbl.get_text().replace("Ideation Core", "Ideation\nCore")
                          for lbl in ax_b.get_xticklabels()], fontsize=FS_TICK - 0.5)

    (gv_d, ko_d, yl_d, t_d), (gv_e, ko_e, yl_e, _), (gv_p, _, _, _), (gv_f, ko_f, yl_f, t_f) = ABL
    two_arm(row2[0], gv_d, ko_d, yl_d, t_d, abl_q[0], abl_res[0][0])
    two_arm_grouped(row2[1], [("Before Refinement", gv_p, abl_q[2], abl_res[2][0]),
                              ("After Refinement", gv_e, abl_q[1], abl_res[1][0])],
                    ko_e, yl_e, "Element Selection")
    two_arm(row2[2], gv_f, ko_f, yl_f, t_f, abl_q[3], abl_res[3][0])

    stage_effects.draw(ax_h, E, adj, pre_keys, "Effect of each stage on each measure")
    ax_h.set_title("Effect of each stage\non each measure", fontsize=FS_TITLE, pad=6)
    for i, ax in enumerate([ax_a, ax_s, ax_b] + row2 + [ax_h]):
        ax.text(-0.16, 1.10, chr(ord("a") + i), transform=ax.transAxes,
                fontsize=16, fontweight="bold", va="top", ha="left")

    fig.savefig(args.out / f"{args.name}.png", dpi=200, bbox_inches="tight")
    fig.savefig(args.out / f"{args.name}.svg", bbox_inches="tight")
    plt.close(fig)
    print(f"wrote {args.out / args.name}.png and .svg")


if __name__ == "__main__":
    main()
