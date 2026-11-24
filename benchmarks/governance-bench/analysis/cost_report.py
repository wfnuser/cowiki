"""Account for measured turns and for the shared M2 replay using saved records."""
import argparse
from collections import Counter
import json
from pathlib import Path
from statistics import mean

METHODS = ("none","rules","single-pass","remote","both")
def read(path):
    return json.loads(path.read_text())
def jsonl(path):
    return list(map(json.loads,path.read_text().splitlines()))
def report(run,ledger,freeze,out):
    events=jsonl(run/"events.jsonl")
    started=next(r["time"] for r in events if r["event"]=="run_start")
    finished=next(r["time"] for r in events if r["event"]=="run_complete")
    trace=jsonl(ledger)[int((run/"ledger-start").read_text()):]
    assert all(not r["calibration"] for r in trace)
    # Save only the trace metadata already whitelisted by the shim, never request bodies.
    fields=("time","arm","result","cli_usage","elapsed_s","request_hash","effort","model","events")
    (out/"turns.jsonl").write_text("".join(json.dumps({k:r[k] for k in fields if k in r},sort_keys=True)+"\n" for r in trace))
    turns=[r for r in trace if r["result"]!="recovery_hit"]
    metadata=read(freeze)
    summary={"elapsed_s":finished-started,"interval_start":started,"interval_end":finished,"arms":{}}
    for m in METHODS:
        d=read(run/(m+".json"))
        cost=d["cost"]
        if m=="both":
            # A replay has no provider cost of its own. Check shared keys, including abstentions.
            upstream=read(run/"remote.json")
            def keys(rep): return {r["id"]:r["outcome"]["cache_key"] for r in rep["cases"]}
            assert keys(upstream)==keys(d) and cost["http_calls"]==0
        attempts=[r for r in turns if r.get("arm")==m]
        measured=[r["cli_usage"] for r in attempts if "cli_usage" in r]
        clean=[r for r in attempts if r["result"]=="completed"]
        assert all(sum(evt=="turn.completed" for evt,_ in r["events"])==1 and
                   all(item in ("","error","agent_message","reasoning") for _,item in r["events"]) for r in clean)
        overhead=metadata["calibration_input_tokens"].get(m,0)
        times=[e.get("wall_s",0) for e in events if e.get("arm")==m and
               e["event"] in ("arm_complete","arm_interrupted","infrastructure_interrupt")]
        pdata=[u["input_tokens"]+u["output_tokens"] for u in measured]
        summary["arms"][m]={
            "cases":len(d["cases"]),"logical_calls":cost["logical_calls"],"http_calls_in_final_pass":cost["http_calls"],
            "physical_cli_attempts":len(attempts),"measured_turns":len(measured),
            "provider_input":sum(u["input_tokens"] for u in measured),"provider_output":sum(u["output_tokens"] for u in measured),
            "cached_provider_input":sum(u.get("cached_input_tokens",0) for u in measured),
            "overhead_per_call":overhead,"overhead_in_logical_tokens_estimate":overhead*cost["logical_calls"],
            "overhead_in_measured_physical_tokens_estimate":overhead*len(measured),
            "logical_input":cost["prompt_tokens"],"logical_output":cost["completion_tokens"],
            "estimated_case_costs":cost["estimated_cases"],"logical_total_tokens":cost["total_tokens"],
            "physical_seconds_per_call":mean(r["elapsed_s"] for r in attempts) if attempts else None,
            "physical_call_seconds_sum":sum(r["elapsed_s"] for r in attempts),"arm_wall_seconds":sum(times),
            "tool_checked_completions":len(clean),"physical_results":dict(Counter(r["result"] for r in attempts)),
            "over_cap_measured":sum(t>16384 or u["output_tokens"]>768 for u,t in zip(measured,pdata)),
            "max_measured_total":max(pdata,default=0),"max_measured_output":max((u["output_tokens"] for u in measured),default=0),
            "outcome_errors":dict(Counter(r["outcome"]["error"] for r in d["cases"] if r["outcome"]["error"]))}
    (out/"cost.json").write_text(json.dumps(summary,indent=2,sort_keys=True)+"\n")
if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    for k in ("run","ledger","freeze","out"): p.add_argument("--"+k,type=Path,required=True)
    a=p.parse_args()
    report(a.run,a.ledger,a.freeze,a.out)
