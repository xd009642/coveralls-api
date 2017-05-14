extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate md5;
extern crate deflate;
extern crate curl;

use std::io;
use std::path::Path;
use std::fs::File;
use std::io::prelude::*;
use std::collections::HashMap;
use serde::ser::{Serialize, Serializer, SerializeStruct};
use curl::easy::{Easy, Form};
use deflate::deflate_bytes_gzip;


/// Representation of branch data
pub struct BranchData {
    pub line_number: usize,
    pub block_name: usize,
    pub branch_number: usize,
    pub hits: usize,
}

/// Expands the line map into the form expected by coveralls (includes uncoverable lines)
fn expand_lines(lines: &HashMap<usize, usize>, line_count: usize) -> Vec<Option<usize>> {
    (0..line_count).map(|x| match lines.get(&(x+1)){
        Some(x) => Some(*x),
        None => None
    }).collect::<Vec<Option<usize>>>()
}

/// Expands branch coverage into the less user friendly format used by coveralls -
/// an array with the contents of the structs repeated one after another in an array.
fn expand_branches(branches: &Vec<BranchData>) -> Vec<usize> {
    branches.iter()
            .flat_map(|x| vec![x.line_number, x.block_name, x.branch_number, x.hits])
            .collect::<Vec<usize>>()
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


impl Source {
    /// Creates a source description for a given file.
    /// display_name: Name given to the source file
    /// repo_path - Path to file relative to repository root 
    /// path - absolute path on file system
    /// lines - map of line numbers to hits
    /// branches - optional, vector of branches in code
    pub fn new(repo_path: &Path, 
           path: &Path, 
           lines: &HashMap<usize, usize>, 
           branches: &Option<Vec<BranchData>>,
           include_source: bool) -> Result<Source, io::Error> {
        
        let mut code = File::open(path)?;
        let mut content = String::new();
        code.read_to_string(&mut content)?;
        let src = if include_source {
            Some(content.clone())
        } else {
            None
        };

        let brch = match branches {
            &Some(ref b) => Some(expand_branches(&b)),
            &None => None,
        };
        let line_count = content.lines().count();
        Ok(Source {
            name: repo_path.to_str().unwrap_or("").to_string(),
            source_digest: format!("{:x}", md5::compute(content)),
            coverage:  expand_lines(lines, line_count),
            branches: brch,
            source:src,
        })
    }
}

#[derive(Serialize)]
pub struct Head {
    pub id: String,
    pub author_name: String,
    pub author_email: String,
    pub committer_name: String,
    pub committer_email: String,
    pub message: String,
}

#[derive(Serialize)]
pub struct Remote {
    pub name: String,
    pub url: String,
}

#[derive(Serialize)]
pub struct GitInfo {
    pub head: Head,
    pub branch: String,
    pub remotes: Vec<Remote>
}

/// Reports the status of a coveralls report upload.
pub enum UploadStatus {
    /// Upload failed. Includes HTTP error code.
    Failed(u32),
    /// Upload succeeded
    Succeeded,
    /// Waiting for response from server or timeout
    Pending,
    /// Retrieving response code resulted in a CURL error no way of determining
    /// success
    Unknown
}

/// Continuous Integration services and the string identifiers coveralls.io
/// uses to present them.
#[derive(Debug, Clone)]
pub enum CiService {
    Travis,
    TravisPro,
    Circle,
    Semaphore,
    Jenkins,
    Codeship,
    /// Other Ci Service, coveralls-ruby is a valid input which gives same features
    /// as travis for coveralls users.
    Other(String)
}

impl CiService {
    fn value<'a>(&'a self) -> &'a str {
        use CiService::*;
        // Only travis and ruby have special features but the others might gain
        // those features in future so best to put them all for now.
        match *self {
            Travis => "travis-ci",
            TravisPro => "travis-pro",
            Other(ref x) => x.as_str(),
            Circle => "circle-ci",
            Semaphore => "semaphore",
            Jenkins => "jenkins",
            Codeship => "codeship",
        }
    }
}

/// Service's are used for CI integration. Coveralls current supports
/// * travis ci
/// * travis pro
/// * circleCI
/// * Semaphore
/// * JenkinsCI
/// * Codeship
#[derive(Clone)]
pub struct Service {
    pub service_name: CiService,
    pub service_job_id: String,
}

/// Repo tokens are alternatives to Services and involve a secret token on coveralls
#[derive(Clone)]
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
    /// Git commit SHA
    commit: Option<String>,
    /// Git information
    git: Option<GitInfo>,
    /// Handle for curl communications
    handle: Easy,
}


impl CoverallsReport {
    /// Create new coveralls report given a unique identifier which allows 
    /// coveralls to identify the user and project
    pub fn new(id: Identity) -> CoverallsReport {
        CoverallsReport {
            id: id,
            source_files: Vec::new(),
            commit: None,
            git: None,
            handle: Easy::new(),
        }
    }

    /// Add generated source data to coveralls report.
    pub fn add_source(&mut self, source: Source) {
        self.source_files.push(source);
    }
    
    /// Sets the commit ID. Overrides more detailed git info
    pub fn set_commit(&mut self, commit: &str) {
        self.commit = Some(commit.to_string());
        self.git = None;
    }

    /// Set detailed git information, overrides commit ID if set.
    pub fn set_detailed_git_info(&mut self, git: GitInfo) {
        self.git = Some(git);
        self.commit = None;
    }

    /// Send report to the coveralls.io directly. For coveralls hosted on other
    /// platforms see send_to_endpoint
    pub fn send_to_coveralls(&mut self) -> Result<(), curl::Error> {
        self.send_to_endpoint("https://coveralls.io/api/v1/jobs")
    }

    /// Sends coveralls report to the specified url
    pub fn send_to_endpoint(&mut self, url: &str) -> Result<(), curl::Error> {
        let body = match serde_json::to_vec(&self) {
            Ok(body) => body,
            Err(e) => panic!("Error {}", e),
        };      
        
        let body = deflate_bytes_gzip(&body);
        self.handle.url(url).unwrap();
        let mut form = Form::new();
        form.part("json_file")
            .content_type("gzip/json")
            .buffer("report", body)
            .add()
            .unwrap();
       self.handle.httppost(form).unwrap();
       self.handle.perform()
    }

    pub fn upload_status(&mut self) -> UploadStatus {
        match self.handle.response_code() {
            Ok(200) => UploadStatus::Succeeded,
            Ok(0) => UploadStatus::Pending,
            Ok(x) => UploadStatus::Failed(x),
            _ => UploadStatus::Unknown,
        }
    }
}


impl Serialize for CoverallsReport {
    
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
        let size = 1 + match self.id {
            Identity::RepoToken(_) => 1 + self.commit.is_some() as usize,
            Identity::ServiceToken(_) => 2 + self.commit.is_some() as usize,
        };
        let mut s = serializer.serialize_struct("CoverallsReport", size)?;
        match self.id {
            Identity::RepoToken(ref r) => {
                s.serialize_field("repo_token", &r)?;
            },
            Identity::ServiceToken(ref serv) => {
                s.serialize_field("service_name", serv.service_name.value())?;
                s.serialize_field("service_job_id", &serv.service_job_id)?;
            },
        }
        if let Some(ref sha) = self.commit {
            s.serialize_field("commit_sha", &sha)?;
        }
        if let Some(ref git) = self.git {
            s.serialize_field("git", &git)?;
        }
        s.serialize_field("source_files", &self.source_files)?;
        s.end()
    }
}


#[cfg(test)]
mod tests {

    use std::collections::HashMap;
    use ::*;

    #[test]
    fn test_expand_lines() {
        let line_count = 10;
        let mut example: HashMap<usize, usize> = HashMap::new();
        example.insert(5, 1);
        example.insert(6, 1);
        example.insert(8, 2);
        
        let expected = vec![None, None, None, None, Some(1), Some(1), None, Some(2), None, None];

        assert_eq!(expand_lines(&example, line_count), expected);
    }

    #[test]
    fn test_branch_expand() {
        let b1 = BranchData {
            line_number: 3,
            block_name: 1,
            branch_number: 1,
            hits: 1,
        };
        let b2 = BranchData {
            line_number:4,
            block_name: 1,
            branch_number: 2,
            hits: 0
        };

        let v = vec![b1, b2];
        let actual = expand_branches(&v);
        let expected = vec![3,1,1,1,4,1,2,0];
        assert_eq!(actual, expected);    
    }

}
