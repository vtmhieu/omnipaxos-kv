import os
import numpy as np
import matplotlib.pyplot as plt
import matplotlib.ticker as mticker


# make an output directory for the plots
OUTPUT_DIR = "./plots"
os.makedirs(OUTPUT_DIR, exist_ok=True)
COLORS = ["#1D9E75", "#378ADD", "#D85A30"]

plt.rcParams.update({
    "figure.facecolor": "white",
    "axes.facecolor":   "white",
    "axes.grid":        True,
    "grid.color":       "#e0ddd6",
    "grid.linewidth":   0.6,
    "axes.spines.top":  False,
    "axes.spines.right":False,
    "font.family":      "sans-serif",
    "font.size":        11,
})
 
# the data from the json files
DATA = {
    "Adaptive Deadline": {
        "High quality":   [
            {"avg_latency_ms": 9,  "fastpath_ratio": 1.000, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 11, "fastpath_ratio": 0.957, "throughput_ops_per_sec": 75.03},
            {"avg_latency_ms": 10, "fastpath_ratio": 0.964, "throughput_ops_per_sec": 74.95},
            {"avg_latency_ms": 7,  "fastpath_ratio": 1.000, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 8,  "fastpath_ratio": 0.976, "throughput_ops_per_sec": 75.04},
            {"avg_latency_ms": 14, "fastpath_ratio": 1.000, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 9,  "fastpath_ratio": 1.000, "throughput_ops_per_sec": 75.03},
            {"avg_latency_ms": 12, "fastpath_ratio": 0.786, "throughput_ops_per_sec": 75.04},
            {"avg_latency_ms": 5,  "fastpath_ratio": 1.000, "throughput_ops_per_sec": 74.98},
            {"avg_latency_ms": 9,  "fastpath_ratio": 0.993, "throughput_ops_per_sec": 75.04},
        ],
        "Medium quality": [
            {"avg_latency_ms": 8,  "fastpath_ratio": 0.883, "throughput_ops_per_sec": 74.94},
            {"avg_latency_ms": 11, "fastpath_ratio": 0.835, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 19, "fastpath_ratio": 0.989, "throughput_ops_per_sec": 75.04},
            {"avg_latency_ms": 15, "fastpath_ratio": 0.961, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 13, "fastpath_ratio": 0.676, "throughput_ops_per_sec": 74.89},
            {"avg_latency_ms": 9,  "fastpath_ratio": 0.945, "throughput_ops_per_sec": 74.49},
            {"avg_latency_ms": 14, "fastpath_ratio": 0.965, "throughput_ops_per_sec": 74.94},
            {"avg_latency_ms": 15, "fastpath_ratio": 0.972, "throughput_ops_per_sec": 75.04},
            {"avg_latency_ms": 15, "fastpath_ratio": 0.972, "throughput_ops_per_sec": 75.04},
            {"avg_latency_ms": 13, "fastpath_ratio": 0.954, "throughput_ops_per_sec": 74.99},
        ],
        "Low quality": [
            {"avg_latency_ms": 56, "fastpath_ratio": 0.700, "throughput_ops_per_sec": 74.95},
            {"avg_latency_ms": 10, "fastpath_ratio": 0.271, "throughput_ops_per_sec": 75.05},
            {"avg_latency_ms": 12, "fastpath_ratio": 0.390, "throughput_ops_per_sec": 75.04},
            {"avg_latency_ms": 9,  "fastpath_ratio": 0.398, "throughput_ops_per_sec": 74.44},
            {"avg_latency_ms": 82, "fastpath_ratio": 0.856, "throughput_ops_per_sec": 73.74},
            {"avg_latency_ms": 44, "fastpath_ratio": 0.483, "throughput_ops_per_sec": 74.53},
            {"avg_latency_ms": 31, "fastpath_ratio": 0.025, "throughput_ops_per_sec": 75.04},
            {"avg_latency_ms": 70, "fastpath_ratio": 0.745, "throughput_ops_per_sec": 74.74},
            {"avg_latency_ms": 9,  "fastpath_ratio": 0.377, "throughput_ops_per_sec": 68.89},
            {"avg_latency_ms": 16, "fastpath_ratio": 0.064, "throughput_ops_per_sec": 74.49},
        ],
    },
    "Clock Config": {
        "High quality": [
            {"avg_latency_ms": 7,   "fastpath_ratio": 1.0000, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 5,   "fastpath_ratio": 1.0000, "throughput_ops_per_sec": 75.04},
            {"avg_latency_ms": 9,   "fastpath_ratio": 0.9993, "throughput_ops_per_sec": 75.04},
            {"avg_latency_ms": 9,   "fastpath_ratio": 0.9973, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 10,  "fastpath_ratio": 0.9947, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 7,   "fastpath_ratio": 1.0000, "throughput_ops_per_sec": 75.04},
            {"avg_latency_ms": 7,   "fastpath_ratio": 1.0000, "throughput_ops_per_sec": 75.04},
            {"avg_latency_ms": 6,   "fastpath_ratio": 1.0000, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 7,   "fastpath_ratio": 1.0000, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 6,   "fastpath_ratio": 1.0000, "throughput_ops_per_sec": 75.04},
        ],
        "Medium quality": [
            {"avg_latency_ms": 21,  "fastpath_ratio": 0.404, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 23,  "fastpath_ratio": 1.000, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 10,  "fastpath_ratio": 0.700, "throughput_ops_per_sec": 75.03},
            {"avg_latency_ms": 10,  "fastpath_ratio": 0.795, "throughput_ops_per_sec": 75.05},
            {"avg_latency_ms": 9,   "fastpath_ratio": 0.818, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 12,  "fastpath_ratio": 0.007, "throughput_ops_per_sec": 75.04},
            {"avg_latency_ms": 103, "fastpath_ratio": 1.000, "throughput_ops_per_sec": 74.84},
            {"avg_latency_ms": 16,  "fastpath_ratio": 0.641, "throughput_ops_per_sec": 75.04},
            {"avg_latency_ms": 13,  "fastpath_ratio": 0.696, "throughput_ops_per_sec": 74.59},
            {"avg_latency_ms": 9,   "fastpath_ratio": 0.831, "throughput_ops_per_sec": 72.34},
        ],
        "Low quality": [
            {"avg_latency_ms": 25,  "fastpath_ratio": 0.419, "throughput_ops_per_sec": 66.60},
            {"avg_latency_ms": 28,  "fastpath_ratio": 0.331, "throughput_ops_per_sec": 73.59},
            {"avg_latency_ms": 26,  "fastpath_ratio": 0.286, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 30,  "fastpath_ratio": 0.344, "throughput_ops_per_sec": 74.84},
            {"avg_latency_ms": 31,  "fastpath_ratio": 0.360, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 31,  "fastpath_ratio": 0.360, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 68,  "fastpath_ratio": 0.826, "throughput_ops_per_sec": 74.59},
            {"avg_latency_ms": 31,  "fastpath_ratio": 0.328, "throughput_ops_per_sec": 74.94},
            {"avg_latency_ms": 33,  "fastpath_ratio": 0.324, "throughput_ops_per_sec": 74.89},
            {"avg_latency_ms": 29,  "fastpath_ratio": 0.356, "throughput_ops_per_sec": 74.89},
        ],
    },
    "Clock Drift": {
        "Low drift (2.0)": [
            {"avg_latency_ms": 7,  "fastpath_ratio": 1.0000, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 5,  "fastpath_ratio": 1.0000, "throughput_ops_per_sec": 75.04},
            {"avg_latency_ms": 9,  "fastpath_ratio": 0.9993, "throughput_ops_per_sec": 75.04},
            {"avg_latency_ms": 9,  "fastpath_ratio": 0.9973, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 10, "fastpath_ratio": 0.9947, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 7,  "fastpath_ratio": 1.0000, "throughput_ops_per_sec": 75.04},
            {"avg_latency_ms": 7,  "fastpath_ratio": 1.0000, "throughput_ops_per_sec": 75.04},
            {"avg_latency_ms": 6,  "fastpath_ratio": 1.0000, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 7,  "fastpath_ratio": 1.0000, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 6,  "fastpath_ratio": 1.0000, "throughput_ops_per_sec": 75.04},
        ],
        "Medium drift (25.0)": [
            {"avg_latency_ms": 16, "fastpath_ratio": 0.748, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 84, "fastpath_ratio": 1.000, "throughput_ops_per_sec": 74.84},
            {"avg_latency_ms": 7,  "fastpath_ratio": 0.891, "throughput_ops_per_sec": 73.14},
            {"avg_latency_ms": 11, "fastpath_ratio": 0.817, "throughput_ops_per_sec": 74.94},
            {"avg_latency_ms": 12, "fastpath_ratio": 0.900, "throughput_ops_per_sec": 74.69},
            {"avg_latency_ms": 10, "fastpath_ratio": 0.960, "throughput_ops_per_sec": 73.64},
            {"avg_latency_ms": 15, "fastpath_ratio": 0.137, "throughput_ops_per_sec": 75.04},
            {"avg_latency_ms": 6,  "fastpath_ratio": 1.000, "throughput_ops_per_sec": 75.04},
            {"avg_latency_ms": 7,  "fastpath_ratio": 0.802, "throughput_ops_per_sec": 73.74},
            {"avg_latency_ms": 11, "fastpath_ratio": 0.728, "throughput_ops_per_sec": 75.04},
        ],
        "High drift (100.0)": [
            {"avg_latency_ms": 33, "fastpath_ratio": 0.977, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 37, "fastpath_ratio": 0.353, "throughput_ops_per_sec": 74.89},
            {"avg_latency_ms": 31, "fastpath_ratio": 0.238, "throughput_ops_per_sec": 74.94},
            {"avg_latency_ms": 26, "fastpath_ratio": 0.954, "throughput_ops_per_sec": 74.34},
            {"avg_latency_ms": 19, "fastpath_ratio": 0.498, "throughput_ops_per_sec": 72.74},
            {"avg_latency_ms": 25, "fastpath_ratio": 0.277, "throughput_ops_per_sec": 74.98},
            {"avg_latency_ms": 33, "fastpath_ratio": 0.399, "throughput_ops_per_sec": 74.89},
            {"avg_latency_ms": 31, "fastpath_ratio": 0.657, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 27, "fastpath_ratio": 0.234, "throughput_ops_per_sec": 74.99},
            {"avg_latency_ms": 21, "fastpath_ratio": 0.920, "throughput_ops_per_sec": 74.95},
        ],
    },
}


 
def comparison_bar_plot(exp_a: str, exp_b: str, metric: str, ylabel: str, filename: str, pct: bool = False):
    
    tiers = list(DATA[exp_a].keys())
    x     = np.arange(len(tiers))
    width = 0.35
    
    # get the mean and standar deviations for both
    # for the plotting
    means_a, stds_a = [], []
    means_b, stds_b = [], []
    
    # get the values from both sites
    for tier in tiers:
        vals_a = np.array([r[metric] for r in DATA[exp_a][tier]])
        vals_b = np.array([r[metric] for r in DATA[exp_b][tier]])
        if pct:
            vals_a = vals_a * 100
            vals_b = vals_b * 100
        means_a.append(vals_a.mean())
        stds_a.append(vals_a.std())
        means_b.append(vals_b.mean())
        stds_b.append(vals_b.std())
 
    fig, ax = plt.subplots(figsize=(8, 5))
    
    # plot with barplot
    bars_a = ax.bar(x - width / 2, means_a, width, yerr=stds_a, capsize=5,
                    color=COLORS[0], label=exp_a, zorder=2)
    bars_b = ax.bar(x + width / 2, means_b, width, yerr=stds_b, capsize=5,
                    color=COLORS[1], label=exp_b, zorder=2)
 
    ax.bar_label(bars_a, labels=[f"{m:.1f}{'%' if pct else ''}" for m in means_a], padding=3, fontsize=9)
    ax.bar_label(bars_b, labels=[f"{m:.1f}{'%' if pct else ''}" for m in means_b], padding=3, fontsize=9)
 
    ax.set_xticks(x)
    ax.set_xticklabels(tiers)
    ax.set_ylabel(ylabel)
    ax.set_title(f"{exp_a} vs {exp_b} — {ylabel}\n(mean ± std across 10 runs)")
    ax.legend(frameon=False)
 
    if pct:
        ax.set_ylim(0, 120)
        ax.yaxis.set_major_formatter(mticker.PercentFormatter())
 
    fig.tight_layout()
    out = os.path.join(OUTPUT_DIR, f"{filename}.png")
    fig.savefig(out, dpi=150)
    plt.close(fig)
    print(f"Saved: {out}")


def main():
    comparison_bar_plot(
        "Adaptive Deadline", "Clock Config",
        "fastpath_ratio", "Fast-path ratio (%)",
        "comparison_fastpath_adaptive_vs_clockconfig",
        pct=True,
    )
 
 
if __name__ == "__main__":
    main()
 