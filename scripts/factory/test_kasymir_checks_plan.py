"""Recipe checks are not product QA; fixture reports only test the strict gate."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
import kasymir_checks_plan as recipes


class RecipeTests(unittest.TestCase):
    def run_report(self, report, kind='jest'):
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/'report.json'
            child="require('node:fs').writeFileSync("+json.dumps(str(path))+","+json.dumps(json.dumps(report))+")"
            command=recipes.strict(['node','-e',child],kind)
            command[2]=command[2].replace('/output/nm-test-results.json',str(path))
            return subprocess.run(command,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode

    @unittest.skipUnless(__import__('shutil').which('node'),'Requires installed Node SDK')
    def test_json_reports_reject_empty_skipped_and_failed_tests(self):
        good={'success':True,'numTotalTests':2,'numPassedTests':2,'numPendingTests':0}
        self.assertEqual(self.run_report(good),0)
        for patch in ({'numTotalTests':0,'numPassedTests':0},{'numPendingTests':1},{'numPassedTests':1},{'success':False}):
            self.assertNotEqual(self.run_report(dict(good,**patch)),0)

    @unittest.skipUnless(__import__('shutil').which('node'),'Requires installed Node SDK')
    def test_browser_reports_reject_skips_expected_failures_and_accessibility_findings(self):
        good={'stats':{'expected':1,'unexpected':0,'skipped':0,'flaky':0},'suites':[{'specs':[{'tests':[{'expectedStatus':'passed','results':[{'status':'passed'}]}]}]}]}
        self.assertEqual(self.run_report(good,'playwright'),0)
        for bad in [dict(good,stats={'expected':1,'skipped':1}),dict(good,suites=[]),dict(good,annotations=[{'type':'a11y-finding'}]),dict(good,suites=[{'specs':[{'tests':[{'expectedStatus':'failed','results':[{'status':'failed'}]}]}]}])]:
            self.assertNotEqual(self.run_report(bad,'playwright'),0)

    @unittest.skipUnless(__import__('shutil').which('node'),'Requires installed Node SDK')
    def test_tap_reports_reject_empty_or_skipped_tests(self):
        for text,passed in [('# tests 2\n# pass 2\n# fail 0\n# skipped 0\n',True),('# tests 0\n# pass 0\n',False),('# tests 2\n# pass 1\n# skipped 1\n',False)]:
            command=recipes.strict(['node','-e','process.stdout.write('+json.dumps(text)+')'],'tap')
            result=subprocess.run(command,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
            self.assertEqual(result.returncode==0,passed)


if __name__=='__main__':
    unittest.main()
