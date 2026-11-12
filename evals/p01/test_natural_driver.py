"""Offline transport fixtures; zero provider/DB execution."""
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest
import uuid

FILE=Path(__file__).resolve().parents[2]/'scripts/test_live_natural.py'
spec=importlib.util.spec_from_file_location('natural_driver',FILE)
driver=importlib.util.module_from_spec(spec);spec.loader.exec_module(driver)


class Fake:
    def __init__(self,session):
        self.session=session;self.posts=[];self.failed_post=False;self.allowed=True;self.replies=True;self.status='committed'
        self.run=str(uuid.uuid4());self.row={'id':str(uuid.uuid4()),'run_id':self.run,'status':'settled',
            'usage_unknown':False,'actual_input':100,'actual_output':20,'charged_tokens':120}
    def accounting(self):
        return {'release_id':self.session['release_id'],'individual_id':self.session['individual_id'],
            'campaign_ledger_id':self.session['campaign_ledger_id'],'inventory_complete':True,
            'usage':[self.row] if self.posts else [],'all_campaign_charged_tokens':200 if self.posts else 80,
            'dispatch_allowed':self.allowed,'budget_frozen':False}
    def api(self,method,path,body=None,key=None):
        self.last_http_status=200
        if path=='/state':return {'individual_id':self.session['individual_id'],'version':1}
        if path=='/diagnostics':return {'service_started_at':'2026-11-12T18:00:00Z'}
        if method=='POST':
            self.last_http_status=202
            self.posts.append((body,key))
            if self.failed_post:raise TimeoutError('DO-NOT-EXPORT-PRIVATE')
            return {'operation_id':self.run,'event_id':str(uuid.uuid4())}
        if path.startswith('/operations/'):return {'operation_id':self.run,'status':self.status,
            'error_code':'decision_grounding_failed' if self.status=='failed' else None}
        if path.startswith('/runs/'):
            return {'individual_id':self.session['individual_id'],'run':{'id':self.run},
                'spans':[],'spans_more':False,'payload_policy':'metadata_only'}
        if path.startswith('/source-events/'):
            return {'individual_id':self.session['individual_id'],'event':{'id':path.rsplit('/',1)[1]}}
        return {'items':[{'id':str(uuid.uuid4()),'role':'assistant','text':'先说说今天发生了什么？',
            'occurred_at':'2026-11-12T18:01:00Z','operation_id':self.run}] if self.replies else []}


class NaturalDriver(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.path=Path(self.tmp.name)/'report.json'
        self.session={'release_id':'VISIBLE-NATURAL-01/r1','individual_id':str(uuid.uuid4()),
            'campaign_ledger_id':'fixed-project','conversation_id':str(uuid.uuid4())}
        self.report={'binding':{'version':'fixed'},'turns':[]};self.transport=Fake(self.session)
        self.case={'id':'daily-1','layer':'daily','kind':'short_natural','tester_only_criteria':['NEVER SEND THIS']}
    def run_turn(self,**kw):
        return driver.run_turn(self.session,self.report,self.path,self.case,'今天有点累。',self.transport,**kw)
    def test_sends_only_one_natural_intent_and_records_actual_commit(self):
        row=self.run_turn();self.assertEqual(row['status'],'committed')
        self.assertEqual(self.transport.posts[0][0],{'text':'今天有点累。','conversation_id':self.session['conversation_id']})
        self.assertNotIn('NEVER SEND',str(self.transport.posts));self.assertEqual(row['actual_usage']['charged_tokens'],120)
        self.assertIsNone(row['semantic_verdict'])
        self.assertEqual(row['submit_http_status'],202)
        self.assertEqual(row['execution_trace']['payload_policy'],'metadata_only')
    def test_budget_denial_does_not_dispatch_and_preserves_unknown_cost(self):
        self.transport.allowed=False;row=self.run_turn()
        self.assertEqual(row['status'],'not_dispatched');self.assertEqual(self.transport.posts,[])
        self.assertIsNone(row['actual_usage']);self.assertEqual(len(self.report['turns']),1)
    def test_only_explicit_preflight_resumption_is_allowed(self):
        self.transport.allowed=False;self.run_turn();self.transport.allowed=True;row=self.run_turn()
        self.assertEqual(row['status'],'committed');self.assertEqual(len(row['preflight_attempts']),1)
        self.assertEqual(len(self.transport.posts),1)
    def test_timeout_never_reposts_or_saves_exception_body(self):
        self.transport.failed_post=True;row=self.run_turn()
        self.assertEqual(row['status'],'outcome_unknown');self.assertNotIn('DO-NOT',self.path.read_text())
        with self.assertRaises(ValueError):self.run_turn()
        self.assertEqual(len(self.transport.posts),1)
    def test_next_turn_requires_actual_reply_observation(self):
        self.run_turn();self.case['id']='daily-2'
        with self.assertRaisesRegex(ValueError,'actual_result_review_required'):self.run_turn()
        ack={'turn_id':'daily-1','actual_reply_quote':'invented','observation':'observed','next_reason':'clarify'}
        with self.assertRaisesRegex(ValueError,'quote_not_actual'):self.run_turn(acknowledgement=ack)
    def test_committed_without_message_is_not_invented(self):
        self.transport.replies=False;row=self.run_turn()
        self.assertEqual(row['replies'],[]);self.assertEqual(row['error_code'],'committed_without_reply')
    def test_runtime_failure_and_paid_usage_retained_without_semantic_inference(self):
        self.transport.status='failed';self.transport.replies=False;row=self.run_turn()
        self.assertEqual(row['status'],'failed');self.assertEqual(row['replies'],[])
        self.assertEqual(row['terminal_operation']['error_code'],'decision_grounding_failed')
        self.assertEqual(row['actual_usage']['charged_tokens'],120);self.assertIsNone(row['semantic_verdict'])
    def test_unknown_usage_is_null_and_duplicate_reservations_rejected(self):
        row=copy.deepcopy(self.transport.row);row['usage_unknown']=True;row['actual_input']=None
        result=driver.usage_summary([row],row['run_id']);self.assertIsNone(result['charged_tokens'])
        self.assertEqual(result['observed_ledger_charges'],120)
        with self.assertRaisesRegex(ValueError,'duplicate_reservation'):driver.usage_summary([row,row])
    def test_missing_run_usage_never_means_zero(self):
        result=driver.usage_summary([],self.transport.run)
        self.assertFalse(result['usage_complete']);self.assertIsNone(result['charged_tokens'])
    def test_settled_conservative_charge_does_not_become_known_actual_usage(self):
        row={**self.transport.row,'usage_unknown':True,'actual_input':None,'actual_output':None,'reserved_tokens':120}
        self.assertTrue(driver.billing_settled([row]))
        self.assertIsNone(driver.usage_summary([row])['charged_tokens'])
        self.assertFalse(driver.billing_settled([{**row,'status':'dispatched'}]))
        self.assertFalse(driver.billing_settled([{**row,'charged_tokens':0}]))
        self.assertFalse(driver.billing_settled([{k:v for k,v in row.items() if k!='reserved_tokens'}]))
    def test_suspended_provider_never_enters_transport(self):
        p=Path(self.tmp.name)/'session.json';p.write_text('{"dispatch_suspended_reason":"provider mismatch"}');p.chmod(0o600)
        with self.assertRaisesRegex(ValueError,'dispatch_suspended'):
            driver.session_from_private(p,Path(self.tmp.name)/'not-read-plan')
    def test_model_boundary_retains_identity_cost_and_old_turn(self):
        old={'individual_id':'one','campaign_ledger_id':'ledger','release_id':'glm'}
        new={**old,'release_id':'deepseek'};report={'binding':old,'turns':[{'accounting_after':{'tokens':120}}]}
        with self.assertRaisesRegex(ValueError,'explicit_model_boundary'):driver.adopt_model_boundary(report,new,{})
        s={'model_boundary':{'previous_binding_sha256':driver.binding_hash(old),'new_binding_sha256':driver.binding_hash(new),'released_by':'Root','reason':'explicit user provider correction'}}
        driver.adopt_model_boundary(report,new,s)
        self.assertEqual(report['turns'][0]['accounting_after']['tokens'],120)
        self.assertEqual(report['model_boundaries'][0]['old_binding'],old)
        with self.assertRaisesRegex(ValueError,'identity_change'):
            driver.adopt_model_boundary(report,{**new,'individual_id':'different'},s)


if __name__=='__main__':unittest.main()
