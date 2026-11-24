"""Render the registered estimates without running a method or recomputing a score."""
import argparse
import csv
from pathlib import Path

NAMES={"none":"None","rules":"Rules","single-pass":"Single-pass","remote":"Verifier only","both":"Combined M2"}
QUALITY=("precision","recall","f1","repair","false_alarm","unintended")
COST=("calls_per_case","tokens_per_case","seconds_per_case","overhead_per_case")
def row(*values):
    return "| "+" | ".join(values)+" |"
def table(*cols):
    return [row(*cols),row(*(["---"]*len(cols)))]
def fmt(x,scale=100):
    return f"{float(x)*scale:,.2f}" if x else "—"
def cell(r,scale=100,paired=False):
    if paired:
        frac=f"{r['m2_num']}/{r['m2_den']} vs {r['baseline_num']}/{r['baseline_den']}"
    else:
        n=float(r['numerator'])
        n=f"{n:g}" if n.is_integer() else f"{n:.2f}"
        frac=f"{n}/{r['denominator']}"
    return f"{fmt(r['estimate'],scale)} [{fmt(r['low'],scale)}, {fmt(r['high'],scale)}] ({frac})"
def render(output):
    with (output/"intervals.csv").open() as f:
        data={(r["stratum"],r["arm"],r["metric"]):r for r in csv.DictReader(f)}
    with (output/"paired-differences.csv").open() as f:
        pairs={(r["stratum"],r["metric"]):r for r in csv.DictReader(f)}
    groups=sorted({k[0] for k in data},key=lambda v:(v!="all",v.startswith("code/"),v))
    lines=["# Held-out statistical appendix","",
           "Quality is in percent. Cells show estimate [95% family-cluster bootstrap interval] (numerator/denominator). F1 uses 2TP/(2TP+FP+FN). Dashes mark undefined ratios. The electronic tables include denominator-bearing family counts and the number of defined draws out of 10,000. [Estimates](intervals.csv) · [Paired contrasts](paired-differences.csv)",""]
    for group in groups:
        metrics=QUALITY[:3] if group.startswith("code/") else QUALITY
        lines+=["## "+group.replace("type/","Primary challenge · ").replace("code/","Finding code · ").replace("_"," "),"",
                *table("Arm",*({"f1":"F1","false_alarm":"False alarm","unintended":"Unintended"}.get(k,k.title()) for k in metrics))]
        for m in NAMES:
            lines.append(row(NAMES[m],*(cell(data[group,m,k]) for k in metrics)))
        lines+=["","### Paired differences in percentage points","",*table("Outcome","Combined M2 minus single-pass [95% interval] (M2 vs single-pass)")]
        for k in ("f1","repair","false_alarm","unintended"):
            if (group,k) in pairs:
                lines.append(row(k.replace("_"," "),cell(pairs[group,k],paired=True)))
        if not group.startswith("code/"):
            lines+=["","### Cost per edit","",*table("Arm","Logical calls","Model tokens","Execution seconds","Included overhead tokens (estimated)")]
            for m in NAMES:
                lines.append(row(NAMES[m],*(cell(data[group,m,k],scale=1) for k in COST)))
        lines+=[""]
    (output/"TABLES.md").write_text("\n".join(lines))
if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("output",type=Path)
    render(p.parse_args().output)
