// R3 release regressions, including the historical plan consumer. Migration v102 is v110 on main.
use nexusmind::db::{qa_coverage as qa, migrations};
use rusqlite::{Connection, types::Value as SqlValue};
use serde_json::{json, Value};
fn db() -> Connection {
    let c=Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE organizations(id TEXT PRIMARY KEY); INSERT INTO organizations VALUES('o');").unwrap();
    qa::migrate(&c).unwrap(); c
}
fn cfg()->Value { json!({"project_id":"p","application":"web","environment":"staging"}) }
fn sc(status:&str)->Value { json!({"key":"cart-total","description":"Observed cart total","status":status,"evidence":["Observed UI result"]}) }
fn result(status:&str)->Value { json!({"scenarios":[sc(status)],"findings":[]}) }
fn final_fact(c:&Connection,run:&str,status:&str) { qa::persist_result(c,"o","a",run,"t1",&cfg(),&result(status),&[]).unwrap(); }
fn checkpoint(c:&Connection,run:&str,status:&str)->bool { qa::persist_checkpoint(c,"o","a",run,"t1",&cfg(),&sc(status),&[]).unwrap() }
fn snapshot(c:&Connection,table:&str)->Vec<Vec<String>> {
    let mut stmt=c.prepare(&format!("SELECT * FROM {table} ORDER BY id")).unwrap();
    let n=stmt.column_count();
    stmt.query_map([],|r| (0..n).map(|i| r.get::<_,SqlValue>(i).map(|v|format!("{v:?}"))).collect()).unwrap().collect::<rusqlite::Result<_>>().unwrap()
}
fn inventory(c:&Connection)->(String,i64,String) {
    c.query_row("SELECT last_status,observation_order,last_tested_at FROM qa_scenario_inventory WHERE scenario_key='cart-total'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap()
}
fn fact(c:&Connection,table:&str,run:&str)->(i64,i64,String) {
    c.query_row(&format!("SELECT id,observation_order,tested_at FROM {table} WHERE run_id=?1"),[run],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap()
}
#[test]
fn r3_a_final_old_new_failed_old_preserves_identity_order_time() {
    let c=db(); final_fact(&c,"old","passed"); let old=fact(&c,"qa_scenario_results","old");
    final_fact(&c,"new","failed"); let new=fact(&c,"qa_scenario_results","new"); assert!(old.1<new.1);
    let rows=snapshot(&c,"qa_scenario_results"); let seq=snapshot(&c,"qa_observation_order"); let inv=inventory(&c);
    final_fact(&c,"old","passed");
    assert_eq!(snapshot(&c,"qa_scenario_results"),rows); assert_eq!(snapshot(&c,"qa_observation_order"),seq); assert_eq!(inventory(&c),inv); assert_eq!(inv.0,"failed");
    println!("R3-A existing inventory PASS: old(id,order,time)={old:?}; new={new:?}; identical facts/sequence/inventory after replay={inv:?}");
}
#[test]
fn r3_a_final_replay_missing_inventory_repairs_newest_failed_fact() {
    let c=db(); final_fact(&c,"old","passed"); final_fact(&c,"new","failed");
    let rows=snapshot(&c,"qa_scenario_results"); let seq=snapshot(&c,"qa_observation_order"); let expected=inventory(&c);
    c.execute("DELETE FROM qa_scenario_inventory",[]).unwrap(); final_fact(&c,"old","passed");
    assert_eq!(snapshot(&c,"qa_scenario_results"),rows); assert_eq!(snapshot(&c,"qa_observation_order"),seq);
    let actual=inventory(&c); println!("R3-A missing inventory: expected={expected:?}; actual={actual:?}; old={:?}; new={:?}; retained identities/order/time unchanged",fact(&c,"qa_scenario_results","old"),fact(&c,"qa_scenario_results","new"));
    assert_eq!(actual,expected,"old final replay must repair inventory from the newest retained observation");
}
#[test]
fn r3_original_same_second_checkpoint_replay_and_missing_inventory_repair() {
    let c=db(); assert!(checkpoint(&c,"old","passed")); final_fact(&c,"new","failed");
    let old=fact(&c,"qa_scenario_checkpoints","old"); let new=fact(&c,"qa_scenario_results","new"); assert_eq!(old.2,new.2,"actual SQLite timestamps, not edited");
    let cp=snapshot(&c,"qa_scenario_checkpoints"); let finals=snapshot(&c,"qa_scenario_results"); let seq=snapshot(&c,"qa_observation_order"); let expected=inventory(&c);
    assert!(!checkpoint(&c,"old","passed")); assert_eq!(inventory(&c),expected);
    c.execute("DELETE FROM qa_scenario_inventory",[]).unwrap(); assert!(!checkpoint(&c,"old","passed"));
    assert_eq!(inventory(&c),expected); assert_eq!(expected.0,"failed");
    assert_eq!(snapshot(&c,"qa_scenario_checkpoints"),cp); assert_eq!(snapshot(&c,"qa_scenario_results"),finals); assert_eq!(snapshot(&c,"qa_observation_order"),seq);
    println!("Original R3 PASS: actual same-second old checkpoint={old:?}; newer final={new:?}; replay + missing repair={expected:?}; all retained facts/sequence unchanged");
}
#[test]
fn r3_b_canonical_v110_global_dates_tie_ids_and_idempotency() {
    let c=Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE qa_scenario_checkpoints(id INTEGER PRIMARY KEY,tested_at TEXT); CREATE TABLE qa_scenario_results(id INTEGER PRIMARY KEY,tested_at TEXT); CREATE TABLE qa_scenario_inventory(id INTEGER PRIMARY KEY); INSERT INTO qa_scenario_checkpoints VALUES(90,'2026-11-02 00:00:00'),(30,'2026-11-01 00:00:00'),(10,'2026-11-01 00:00:00'); INSERT INTO qa_scenario_results VALUES(80,'2026-11-01 00:00:00'),(20,'2026-11-03 00:00:00'),(2,'2026-11-01 00:00:00'); PRAGMA user_version=109;").unwrap();
    migrations::run_v110(&c).unwrap();
    let mut stmt=c.prepare("SELECT source,id,observation_order,tested_at FROM (SELECT 'checkpoint' source,id,observation_order,tested_at FROM qa_scenario_checkpoints UNION ALL SELECT 'result',id,observation_order,tested_at FROM qa_scenario_results) ORDER BY observation_order").unwrap();
    let actual=stmt.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?,r.get::<_,String>(3)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap(); drop(stmt);
    let expected=[("checkpoint",10),("checkpoint",30),("result",2),("result",80),("checkpoint",90),("result",20)];
    for (i,((source,id,order,_),(es,ei))) in actual.iter().zip(expected).enumerate(){assert_eq!((source.as_str(),*id,*order),(es,ei,i as i64+1));}
    assert_eq!(actual.len(),6);
    let before=[snapshot(&c,"qa_scenario_checkpoints"),snapshot(&c,"qa_scenario_results"),snapshot(&c,"qa_observation_order")];
    migrations::run_v110(&c).unwrap();
    assert_eq!(before,[snapshot(&c,"qa_scenario_checkpoints"),snapshot(&c,"qa_scenario_results"),snapshot(&c,"qa_observation_order")]); assert_eq!(c.query_row("PRAGMA user_version",[],|r|r.get::<_,i64>(0)).unwrap(),110);
    println!("R3-B canonical v110 PASS global stream={actual:?}; opposite dates + checkpoint-before-final tie + source id tie + full idempotency");
}
#[test]
fn r3_b_canonical_migration_and_checkpoint_repair_both_dates_and_tie() {
    for (cp_date,final_date,expected_status,expected_date) in [
        ("2026-11-02 00:00:00","2026-11-01 00:00:00","failed","2026-11-02 00:00:00"),
        ("2026-11-01 00:00:00","2026-11-02 00:00:00","passed","2026-11-02 00:00:00"),
        ("2026-11-01 00:00:00","2026-11-01 00:00:00","passed","2026-11-01 00:00:00")
    ] {
        let c=db(); final_fact(&c,"legacy-final","passed"); assert!(checkpoint(&c,"legacy-checkpoint","failed"));
        // Only legacy fixtures receive known dates; runtime replay tests above never edit time.
        c.execute("UPDATE qa_scenario_checkpoints SET tested_at=?1",[cp_date]).unwrap(); c.execute("UPDATE qa_scenario_results SET tested_at=?1",[final_date]).unwrap();
        c.execute_batch("DELETE FROM qa_scenario_inventory; ALTER TABLE qa_scenario_checkpoints DROP COLUMN observation_order; ALTER TABLE qa_scenario_results DROP COLUMN observation_order; ALTER TABLE qa_scenario_inventory DROP COLUMN observation_order; DROP TABLE qa_observation_order; PRAGMA user_version=109;").unwrap();
        migrations::run_v110(&c).unwrap();
        let before=[snapshot(&c,"qa_scenario_checkpoints"),snapshot(&c,"qa_scenario_results"),snapshot(&c,"qa_observation_order")];
        migrations::run_v110(&c).unwrap(); assert!(!checkpoint(&c,"legacy-checkpoint","failed"));
        let actual=inventory(&c); assert_eq!(actual.0,expected_status); assert_eq!(actual.2,expected_date); assert_eq!(actual.1,2);
        assert_eq!(before,[snapshot(&c,"qa_scenario_checkpoints"),snapshot(&c,"qa_scenario_results"),snapshot(&c,"qa_observation_order")]);
        println!("R3-B canonical migrate/repair PASS checkpoint={cp_date}; final={final_date}; inventory={actual:?}; retained facts/order/time unchanged");
    }
}


#[test]
fn r3_a_final_replay_repairs_inventory_without_rewriting_own_attempt_plan() {
    let mut statuses=Vec::new();
    for remove_inventory in [false,true] {
        let c=db();
        let mut old=result("passed");
        old["plan"]=json!([{"key":"cart-total","description":"Observed cart total","reason":"selected scenario"}]);
        qa::persist_result(&c,"o","a","old","t1",&cfg(),&old,&[]).unwrap();
        final_fact(&c,"new","failed");
        let plan_status=|| c.query_row("SELECT status FROM qa_batch_plans WHERE run_id='old' AND attempt_id='t1' AND scenario_key='cart-total'",[],|r|r.get::<_,String>(0)).unwrap();
        assert_eq!(plan_status(),"passed");
        let rows=snapshot(&c,"qa_scenario_results");
        let seq=snapshot(&c,"qa_observation_order");
        let expected=inventory(&c);
        if remove_inventory { c.execute("DELETE FROM qa_scenario_inventory",[]).unwrap(); }
        qa::persist_result(&c,"o","a","old","t1",&cfg(),&old,&[]).unwrap();
        assert_eq!(inventory(&c),expected);
        assert_eq!(snapshot(&c,"qa_scenario_results"),rows);
        assert_eq!(snapshot(&c,"qa_observation_order"),seq);
        let actual=plan_status();
        println!("R3-A own-attempt plan replay: remove_inventory={remove_inventory}; expected=passed; actual={actual}; retained final old=passed; retained final new=failed; inventory={:?}; all final facts and sequence unchanged",inventory(&c));
        statuses.push(actual);
    }
    assert_eq!(statuses,["passed","passed"],"inventory's newer observation must not replace the old attempt's own executed plan outcome");
}
