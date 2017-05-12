extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;

use serde::ser::{Serialize, Serializer, SerializeStruct};
use serde_json::Error;

/// Representation of branch data
pub struct BranchData {
    line_number: usize,
    block_name: usize,
    branch_number: usize,
    hits: usize,
}


/// Struct representing source files and the coverage for coveralls
#[derive(Serialize)]
pub struct Source {
    /// Name of the source file. Represented as path relative to root of repo
    name: String,
    /// MD5 hash of the source file
    source_digest: String,
    /// Coverage for the source. Each element is a line with the following rules:
    /// None - not relevant to coverage
    /// 0 - not covered
    /// 1+ - covered and how often
    coverage: Vec<Option<usize>>,
    /// Branch data for branch coverage.
    #[serde(skip_serializing_if="Option::is_none")]
    branches: Option<Vec<usize>>,
    /// Contents of the source file (Manual Repos on Enterprise only)
    #[serde(skip_serializing_if="Option::is_none")]
    source: Option<String>
}


/// Service's are used for CI integration. Coveralls current supports
/// * travis ci
/// * travis pro
/// * circleCI
/// * Semaphore
/// * JenkinsCI
/// * Codeship
pub struct Service {
    service_name: String,
    service_job_id: String,
}

/// Repo tokens are alternatives to Services and involve a secret token on coveralls
pub enum Identity {
    RepoToken(String),
    ServiceToken(Service)
}


/// Coveralls report struct 
/// for more details: https://coveralls.zendesk.com/hc/en-us/articles/201350799-API-Reference 
pub struct CoverallsReport {
    id: Identity,
    /// List of source files which includes coverage information.
    source_files: Vec<Source>,
}


impl Serialize for CoverallsReport {
    
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
        let mut size = 1 + match self.id {
            Identity::RepoToken(_) => 1,
            Identity::ServiceToken(_) => 2,
        };
        let mut s = serializer.serialize_struct("CoverallsReport", size)?;
        match self.id {
            Identity::RepoToken(ref r) => {
                s.serialize_field("repo_token", &r)?;
            },
            Identity::ServiceToken(ref serv) => {
                s.serialize_field("service_name", &serv.service_name)?;
                s.serialize_field("service_job_id", &serv.service_job_id)?;
            },
        }
        s.serialize_field("source_files", &self.source_files)?;
        s.end()
    }
}

