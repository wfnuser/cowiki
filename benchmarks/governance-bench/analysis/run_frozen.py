"""Run/resume the five registered arms; no final arm may be replaced or rescored."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

REPO=Path(__file__).resolve().parents[3]
PROJECT=REPO.parent
FREEZE=REPO/"benchmarks/governance-bench/results/issue-169/freeze.json"
WORK=PROJECT/"heldout_eval"
SHIM=PROJECT/"llm_shim"
STOP=SHIM/"QUOTA_STOP"
MODES=("none","rules","remote","single-pass","both")
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def log(event, **fields):
    with (WORK/"events.jsonl").open("a") as f:
        f.write(json.dumps(dict(event=event,time=time.time(),**fields),sort_keys=True)+"\n")
        f.flush()
        os.fsync(f.fileno())

def verify():
    freeze=json.loads(FREEZE.read_text())
    for name,expected in freeze["files"].items():
        if sha(PROJECT/name)!=expected:
            raise RuntimeError("frozen file mismatch: "+name)
    for k,v in freeze["verifier_environment"].items():
        if os.environ.get(k)!=v: raise RuntimeError("frozen setting mismatch: "+k)
    for line in (FREEZE.parent/"cache-before.sha256").read_text().splitlines():
        expected,name=line.split("  ",1)
        if sha(PROJECT/"verifier_cache"/name)!=expected: raise RuntimeError("changed pre-freeze cache record")
    return freeze

def wait_for_reset():
    if not STOP.exists(): return
    status=json.loads(STOP.read_text())
    now=time.time()
    reset=status.get("reset_at")
    if not reset:
        sys.path.insert(0,str(SHIM))
        from quota_windows import windows
        reset=max((t for used,t in windows() if used>=100 and t),default=None)
    if not reset:
        raise RuntimeError("quota pause without a verified reset timestamp; resume when account access is confirmed")
    log("quota_wait",reset_at=reset)
    while time.time() < reset+15:
        time.sleep(min(30,reset+15-time.time()))
    # The archived stop is operational state. No configuration or response changes.
    STOP.rename(WORK/("quota-"+str(status["seen"])+".json"))
    log("quota_resume",wait_s=time.time()-now)

def complete(arm, path):
    try:
        data=json.loads(path.read_text())
        return (data["split"]=="held-out" and data["verifier"]==arm and len(data["cases"])==160
                and len({r["id"] for r in data["cases"]})==160 and data["total"]["counts"]["cases"]==160)
    except (FileNotFoundError,KeyError,ValueError,TypeError):
        return False

def reclaim_workers(state):
    """A killed supervisor may have left a live benchmark child producing its report."""
    last={}
    for line in (WORK/"events.jsonl").read_text().splitlines():
        e=json.loads(line)
        if e.get("arm") and e["event"] in ("arm_start","arm_complete","arm_interrupted","infrastructure_interrupt"):
            last[e["arm"]]=e
    for arm,event in last.items():
        if event["event"]!="arm_start": continue
        proc=Path("/proc")/str(event["pid"])
        pending=WORK/(arm+".pending")
        logged=False
        while True:
            try:
                same=(proc/"exe").resolve(strict=True)==(PROJECT/state["binary"]).resolve()
                cmd=(proc/"cmdline").read_bytes().split(b"\0")
                if not same or os.fsencode(pending) not in cmd: break
            except FileNotFoundError:
                break
            if not logged:
                log("recover_worker",arm=arm,pid=event["pid"])
                logged=True
            time.sleep(2)
        if complete(arm,pending):
            pending.rename(WORK/(arm+".json"))
            log("arm_complete",arm=arm,wall_s=time.time()-event["time"],status="recovered")
        else:
            log("infrastructure_interrupt",arm=arm,status="orphan exited",wall_s=time.time()-event["time"])
            time.sleep(125)

def group(modes, state):
    remaining={m for m in modes if not (WORK/(m+".json")).is_file()}
    crashes=0
    while remaining:
        if STOP.exists(): wait_for_reset()
        active={}
        pause=False
        try:
            for arm in sorted(remaining):
                pending=WORK/(arm+".pending")
                if complete(arm,pending):
                    pending.rename(WORK/(arm+".json"));log("recover_completed_report",arm=arm)
                    continue
                # A partial file from an interrupted write contains no final score.
                if pending.exists(): pending.rename(WORK/(arm+f".partial-{time.time_ns()}"))
                cache="replay" if arm=="both" else "record"
                args=[str(PROJECT/state["binary"]),"--split","held-out","--verifier",arm,"--cache",cache,"--out",str(pending)]
                env=dict(os.environ,no_proxy="127.0.0.1,localhost",NO_PROXY="127.0.0.1,localhost")
                with (WORK/(arm+".stderr")).open("ab") as dest:
                    p=subprocess.Popen(args,stdout=dest,stderr=dest,env=env,start_new_session=True)
                active[arm]=(p,time.monotonic())
                log("arm_start",arm=arm,pid=p.pid)
            while active:
                if STOP.exists():
                    pause=True
                    log("quota_interrupt",arms=sorted(active))
                    break
                for arm,(p,start) in list(active.items()):
                    if p.poll() is None: continue
                    wall=time.monotonic()-start
                    pending=WORK/(arm+".pending")
                    if p.returncode in (0,1) and complete(arm,pending):
                        pending.rename(WORK/(arm+".json"))
                        log("arm_complete",arm=arm,wall_s=wall,status=p.returncode)
                    else:
                        crashes+=1
                        log("infrastructure_interrupt",arm=arm,wall_s=wall,status=p.returncode)
                    del active[arm]
                time.sleep(.15)
        finally:
            for arm,(p,start) in active.items():
                if p.poll() is None: os.killpg(p.pid,signal.SIGTERM)
                p.wait()
                log("arm_interrupted",arm=arm,wall_s=time.monotonic()-start)
        remaining={m for m in modes if not (WORK/(m+".json")).is_file()}
        if pause: wait_for_reset()
        elif remaining:
            if crashes>2: raise RuntimeError("repeated infrastructure failure; retain checkpoint")
            # Let any in-flight CLI turn publish its recovery receipt first.
            time.sleep(125)
            log("resume_from_cache",arms=sorted(remaining))

def main():
    os.umask(0o077)
    with (WORK/"run.lock").open("a") as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        if (WORK/"DONE").exists(): raise SystemExit("Frozen run already complete.")
        state=verify()
        marker=WORK/"ledger-start"
        if not marker.exists():
            marker.write_text(str(len((SHIM/"calls.jsonl").read_text().splitlines()))+"\n")
            log("run_start",code=state["code_commit"])
        else:
            log("supervisor_resume")
            reclaim_workers(state)
        def interrupt(*_):
            raise KeyboardInterrupt
        signal.signal(signal.SIGTERM,interrupt)
        for modes in (("none",),("rules",),("remote","single-pass"),("both",)):
            group(modes,state)
        assert all(complete(m,WORK/(m+".json")) for m in MODES)
        log("run_complete")
        (WORK/"DONE").write_text(str(time.time())+"\n")
if __name__=="__main__":
    main()
