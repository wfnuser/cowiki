import unittest
from collections import Counter
from cluster_report import METHODS, interval, summarize

class FamilyBootstrap(unittest.TestCase):
    def test_pairing_carries_cases_and_handles_undefined_strata(self):
        # One nine-case family and one singleton. A family draw replicates
        # every case in that family and uses the same weights for both arms.
        arms={m:{"large":Counter(fn=9,cases=9),"small":Counter(fn=1,cases=1)} for m in METHODS}
        arms["both"]["small"]=Counter(tp=1,cases=1)
        rows,paired=summarize("code/test",arms,["large","small"],[[2,0],[1,1],[0,2]])
        diff=next(r for r in paired if r["metric"]=="f1")
        self.assertAlmostEqual(diff["estimate"],2/11)
        self.assertAlmostEqual(diff["low"],.05*2/11)
        self.assertAlmostEqual(diff["high"],.05*2/11+.95)
        precision=next(r for r in rows if r["arm"]=="both" and r["metric"]=="precision")
        self.assertEqual(precision["defined"],2)
        self.assertEqual((precision["low"],precision["high"]),(1,1))
        self.assertEqual(interval([None,None]),(None,None,0))

if __name__=="__main__":
    unittest.main()
