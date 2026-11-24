"""Paired family-cluster percentile intervals for the existing per-case scorer."""
import argparse
import csv
import json
import math
from collections import Counter, defaultdict
from pathlib import Path
from random import Random

METHODS = ("none", "single-pass", "rules", "remote", "both")
SEED = 16920261207
DRAWS = 10000
METRICS = {
    "precision": (("tp",), ("tp", "fp")),
    "recall": (("tp",), ("tp", "fn")),
    "f1": (("tp","tp"), ("tp","tp","fp","fn")),
    "repair": (("repaired_cases",), ("faulty_cases",)),
    "false_alarm": (("valid_false_alarms",), ("valid_cases",)),
    "unintended": (("unintended_changes",), ("cases",)),
    "calls_per_case": (("logical_calls",), ("cases",)),
    "tokens_per_case": (("tokens",), ("cases",)),
    "seconds_per_case": (("elapsed_s",), ("cases",)),
    "overhead_per_case": (("overhead",), ("cases",)),
}

def read(path):
    return json.loads(path.read_text())

def finding_key(f):
    def span(s):
        return s["path"],s["first"],s["last"]
    return f["code"],span(f["candidate"]),span(f["evidence"])

def case_counts(row, gold, candidate, overhead):
    remaining = {finding_key(f) for f in gold["issues"]}
    detected = defaultdict(Counter)
    for f in row["findings"]:
        key = finding_key(f)
        if key in remaining:
            remaining.remove(key)
            detected[key[0]]["tp"] += 1
        else:
            detected[key[0]]["fp"] += 1
    for key in remaining:
        detected[key[0]]["fn"] += 1
    totals = Counter()
    for c in detected.values():
        totals.update(c)
    valid = not gold["issues"]
    accepted = row["final_text"] in gold["acceptable"]
    usage = row["outcome"]["usage"]
    totals.update(cases=1, valid_cases=int(valid), faulty_cases=int(not valid),
                  repaired_cases=int(not valid and accepted),
                  valid_false_alarms=int(valid and bool(row["findings"])),
                  unintended_changes=int((valid and row["final_text"] != candidate and not accepted)
                    or any(s not in row["final_text"] for s in gold["protected"])),
                  logical_calls=usage["calls"], physical_calls=usage["http_calls"],
                  tokens=usage["prompt_tokens"]+usage["completion_tokens"],
                  elapsed_s=row["elapsed_ns"]/1e9,overhead=overhead*usage["calls"])
    return totals, detected

def quantile(sorted_values, p):
    at = p*(len(sorted_values)-1)
    low = math.floor(at)
    high = math.ceil(at)
    return sorted_values[low]+(at-low)*(sorted_values[high]-sorted_values[low])

def interval(samples):
    values=sorted(x for x in samples if x is not None)
    if not values:
        return None,None,0
    return quantile(values,.025),quantile(values,.975),len(values)

def draw_families(count):
    rng=Random(SEED)
    result=[]
    for _ in range(DRAWS):
        weights=[0]*count
        for _ in range(count):
            weights[rng.randrange(count)]+=1
        result.append(weights)
    return result

def summarize(group, arms, families, weights):
    metrics = ("precision","recall","f1") if group.startswith("code/") else METRICS.keys()
    rows=[]; paired=[]; model_draws={}
    # Shared denominator and zero-count columns recur across many methods.
    columns={}
    def column(vector):
        if vector not in columns:
            terms=[(i,v) for i,v in enumerate(vector) if v]
            columns[vector]=[sum(w[i]*v for i,v in terms) for w in weights] if terms else [0]*len(weights)
        return columns[vector]
    for method in METHODS:
        for metric in metrics:
            nkeys,dkeys=METRICS[metric]
            def vector(keys):
                return tuple(sum(arms[method][f][k] for k in keys) for f in families)
            nv,dv=vector(nkeys),vector(dkeys)
            num,den=sum(nv),sum(dv)
            point=num/den if den else None
            sims=[a/b if b else None for a,b in zip(column(nv),column(dv))]
            lo,hi,n=interval(sims)
            rows.append(dict(stratum=group,arm=method,metric=metric,numerator=num,denominator=den,
                             families_with_denominator=sum(bool(v) for v in dv),estimate=point,low=lo,high=hi,defined=n))
            if method in ("single-pass","both") and metric in ("f1","repair","unintended","false_alarm"):
                if method=="single-pass":
                    model_draws[metric]=(point,sims,num,den)
                else:
                    ref,other,sn,sd=model_draws[metric]
                    lo,hi,n=interval([a-b if a is not None and b is not None else None for a,b in zip(sims,other)])
                    paired.append(dict(stratum=group,metric=metric,m2_num=num,m2_den=den,baseline_num=sn,
                              baseline_den=sd,estimate=point-ref if point is not None and ref is not None else None,
                              low=lo,high=hi,defined=n))
    return rows,paired

def write_csv(path, rows):
    with path.open("w",newline="") as f:
        w=csv.DictWriter(f,fieldnames=list(rows[0]))
        w.writeheader()
        w.writerows(rows)

def report(reports,data,out,overheads):
    out.mkdir(parents=True,exist_ok=True)
    arms={m:read(reports/(m+".json")) for m in METHODS}
    def dataset(name):
        return {x["id"]:x for x in map(json.loads,(data/(name+".jsonl")).read_text().splitlines())}
    gold=dataset("gold");cases=dataset("cases")
    families=sorted({x["family"] for x in cases.values()})
    groups=defaultdict(lambda:{m:defaultdict(Counter) for m in METHODS})
    examples={}
    packets={}
    for m,d in arms.items():
        ids=[r["id"] for r in d["cases"]]
        assert len(ids)==len(set(ids))==len(cases) and set(ids)==set(cases)==set(gold)
        assert d["verifier"]==m and d["split"]==data.name
        hints=[]
        for r in d["cases"]:
            i=r["id"];f=r["family"];g=gold[i]
            assert packets.setdefault(i,r["packet_hash"])==r["packet_hash"]
            assert f==cases[i]["family"] and r["primary_type"]==g["primary_type"]
            c,codes=case_counts(r,g,cases[i]["candidate"],overheads[m])
            for group in ("all","type/"+g["primary_type"]):
                groups[group][m][f].update(c)
            for code,v in codes.items():
                groups["code/"+code][m][f].update(v)
            hints.append((i,f,c,r["outcome"]["error"]))
        total=sum(groups["all"][m].values(),Counter())
        for k in ("tp","fp","fn","faulty_cases","valid_cases","repaired_cases","valid_false_alarms","unintended_changes"):
            assert total[k]==d["total"]["counts"][k],(m,k)
        for t,info in d["by_type"].items():
            subtotal=sum(groups["type/"+t][m].values(),Counter())
            for k,v in info["metrics"]["counts"].items():
                if k!="residual_cases":
                    assert subtotal[k]==v,(m,t,k)
        for code,info in d["detection_by_code"].items():
            subtotal=sum(groups["code/"+code][m].values(),Counter())
            assert all(subtotal[k]==info[k] for k in ("tp","fp","fn"))
        if m in ("remote","both","single-pass"):
            chosen=[]; fams=set()
            def choose(predicates):
                for predicate in predicates:
                    eligible=[(i,f) for i,f,c,e in hints if i not in chosen and predicate(c,e)]
                    if eligible:
                        i,f=min(eligible,key=lambda z:(z[1] in fams,z[0]))
                        chosen.append(i); fams.add(f);return
            any_error=lambda c,e: bool(c["fp"] or c["fn"] or c["faulty_cases"]>c["repaired_cases"] or c["unintended_changes"])
            choose([lambda c,e:c["valid_false_alarms"] or c["unintended_changes"],lambda c,e:c["fp"],any_error])
            choose([lambda c,e:c["fn"]>0 and not e,any_error])
            choose([lambda c,e:c["tp"] and c["faulty_cases"]>c["repaired_cases"],lambda c,e:bool(e),any_error])
            examples[m]=chosen
    weights=draw_families(len(families))
    estimates=[];differences=[]
    for group,values in sorted(groups.items()):
        r,p=summarize(group,values,families,weights)
        estimates+=r;differences+=p
    write_csv(out/"intervals.csv",estimates)
    write_csv(out/"paired-differences.csv",differences)
    (out/"error-ids.json").write_text(json.dumps(examples,indent=2)+"\n")

if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ("reports","data","out"):
        parser.add_argument("--"+name,required=True,type=Path)
    parser.add_argument("--single-overhead",type=int,required=True)
    parser.add_argument("--m2-overhead",type=int,required=True)
    args=parser.parse_args()
    report(args.reports,args.data,args.out,dict(none=0,rules=0,remote=args.m2_overhead,both=args.m2_overhead,
                                               **{"single-pass":args.single_overhead}))
