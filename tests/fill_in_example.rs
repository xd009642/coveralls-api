extern crate coveralls_api;
extern crate serde_json;
use coveralls_api::*;
use std::collections::HashMap;
use std::env;
use std::path::Path;

#[test]
fn test_submission() {
    let mut travis = true;
    let mut secret_key = match std::env::var("TRAVIS_JOB_ID") {
        Ok(key) => key,
        Err(_) => String::new(),
    };
    if secret_key.is_empty() {
        println!("Running using Coveralls key");
        travis = false;
        secret_key = match std::env::var("COVERALLS_KEY") {
            Ok(key) => key,
            Err(_) => panic!("COVERALLS_KEY is not set. Cannot test"),
        };
    } else {
        println!("Running with travis job id");
    }
    let repo_path = Path::new("tests/example/mysource.rs");
    let mut abs_path = env::current_dir().unwrap();
    abs_path.push(repo_path);
    assert!(
        abs_path.exists(),
        "Run the test from project root directory"
    );

    let mut lines: HashMap<usize, usize> = HashMap::new();
    lines.insert(4, 0);
    lines.insert(5, 1);
    lines.insert(6, 2);
    lines.insert(7, 1);

    let source = Source::new(&repo_path, &abs_path.as_path(), &lines, &None, false).unwrap();
    let id = if travis {
        let serv = Service {
            name: CiService::Travis,
            job_id: Some(secret_key),
            number: None,
            build_url: None,
            branch: None,
            pull_request: None,
        };
        Identity::ServiceToken(String::new(), serv)
    } else {
        Identity::RepoToken(secret_key)
    };
    let mut report = CoverallsReport::new(id);
    report.add_source(source);

    report.send_to_coveralls().unwrap();
    loop {
        match report.upload_status() {
            UploadStatus::Failed(x) => panic!("Upload failed! HTTP{}", x),
            UploadStatus::Succeeded => break,
            _ => {}
        }
    }
}
