import json
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import TestCase
from unittest.mock import patch

import run_frozen

class FrozenRunner(TestCase):
    def test_resume_uses_completed_report_and_does_not_redispatch(self):
        with TemporaryDirectory() as tmp:
            work=Path(tmp)
            with patch.object(run_frozen,"WORK",work), patch.object(run_frozen,"STOP",work/"quota"):
                pending=work/"rules.pending"
                pending.write_text(json.dumps({"split":"held-out","verifier":"rules","cases":[{"id":str(i)} for i in range(160)],"total":{"counts":{"cases":160}}}))
                with patch.object(run_frozen.subprocess,"Popen",side_effect=AssertionError("unexpected dispatch")):
                    run_frozen.group(("rules",),{})
                    run_frozen.group(("rules",),{})
                self.assertTrue(run_frozen.complete("rules",work/"rules.json"))
                self.assertFalse(pending.exists())
                self.assertEqual(json.loads((work/"events.jsonl").read_text())["event"],"recover_completed_report")

    def test_quota_latch_is_archived_only_after_reset(self):
        with TemporaryDirectory() as tmp:
            root=Path(tmp); stop=root/"quota"
            stop.write_text(json.dumps({"seen":123,"reset_at":200}))
            with patch.object(run_frozen,"WORK",root),patch.object(run_frozen,"STOP",stop),patch.object(run_frozen.time,"time",return_value=216):
                run_frozen.wait_for_reset()
            self.assertFalse(stop.exists())
            self.assertTrue((root/"quota-123.json").exists())
