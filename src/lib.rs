use curl::easy::{Easy, Form};
use deflate::deflate_bytes_gzip;
use serde::{
    ser::{SerializeStruct, Serializer},
    Deserialize, Serialize,
};
use std::collections::HashMap;
use std::env::var;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::path::Path;
use std::str::FromStr;

/// Representation of branch data
#[derive(
    Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default, Deserialize, Serialize,
)]
pub struct BranchData {
    pub line_number: usize,
    pub block_name: usize,
    pub branch_number: usize,
    pub hits: usize,
}

/// Expands the line map into the form expected by coveralls (includes uncoverable lines)
fn expand_lines(lines: &HashMap<usize, usize>, line_count: usize) -> Vec<Option<usize>> {
    (0..line_count)
        .map(|x| match lines.get(&(x + 1)) {
            Some(x) => Some(*x),
            None => None,
        })
        .collect::<Vec<Option<usize>>>()
}

/// Expands branch coverage into the less user friendly format used by coveralls -
/// an array with the contents of the structs repeated one after another in an array.
fn expand_branches(branches: &Vec<BranchData>) -> Vec<usize> {
    branches
        .iter()
        .flat_map(|x| vec![x.line_number, x.block_name, x.branch_number, x.hits])
        .collect::<Vec<usize>>()
}

/// Struct representing source files and the coverage for coveralls
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default, Deserialize, Serialize)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    branches: Option<Vec<usize>>,
    /// Contents of the source file (Manual Repos on Enterprise only)
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
}

impl Source {
    /// Creates a source description for a given file.
    /// display_name: Name given to the source file
    /// repo_path - Path to file relative to repository root
    /// path - absolute path on file system
    /// lines - map of line numbers to hits
    /// branches - optional, vector of branches in code
    pub fn new(
        repo_path: &Path,
        path: &Path,
        lines: &HashMap<usize, usize>,
        branches: &Option<Vec<BranchData>>,
        include_source: bool,
    ) -> Result<Source, io::Error> {
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
            coverage: expand_lines(lines, line_count),
            branches: brch,
            source: src,
        })
    }
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default, Deserialize, Serialize)]
pub struct Head {
    pub id: String,
    pub author_name: String,
    pub author_email: String,
    pub committer_name: String,
    pub committer_email: String,
    pub message: String,
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default, Deserialize, Serialize)]
pub struct Remote {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default, Deserialize, Serialize)]
pub struct GitInfo {
    pub head: Head,
    pub branch: String,
    pub remotes: Vec<Remote>,
}

/// Reports the status of a coveralls report upload.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Deserialize, Serialize)]
pub enum UploadStatus {
    /// Upload failed. Includes HTTP error code.
    Failed(u32),
    /// Upload succeeded
    Succeeded,
    /// Waiting for response from server or timeout
    Pending,
    /// Retrieving response code resulted in a CURL error no way of determining
    /// success
    Unknown,
}

/// Continuous Integration services and the string identifiers coveralls.io
/// uses to present them.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Deserialize, Serialize)]
pub enum CiService {
    Travis,
    TravisPro,
    Circle,
    Semaphore,
    Jenkins,
    Codeship,
    /// Other Ci Service, coveralls-ruby is a valid input which gives same features
    /// as travis for coveralls users.
    Other(String),
}

impl FromStr for CiService {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let res = match s {
            "travis-ci" => CiService::Travis,
            "travis-pro" => CiService::TravisPro,
            "circle-ci" => CiService::Circle,
            "semaphore" => CiService::Semaphore,
            "jenkins" => CiService::Jenkins,
            "codeship" => CiService::Codeship,
            e => CiService::Other(e.to_string()),
        };
        Ok(res)
    }
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
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct Service {
    /// Name of the CiService
    pub name: CiService,
    /// Job ID
    pub job_id: Option<String>,
    /// Optional service_number
    pub number: Option<String>,
    /// Optional service_build_url
    pub build_url: Option<String>,
    /// Optional service_branch
    pub branch: Option<String>,
    /// Optional service_pull_request
    pub pull_request: Option<String>,
}

impl Service {
    pub fn from_env() -> Option<Self> {
        if var("TRAVIS").is_ok() {
            Some(Self::get_travis_env())
        } else if var("CIRCLECI").is_ok() {
            Some(Self::get_circle_env())
        } else if var("JENKINS_URL").is_ok() {
            Some(Self::get_jenkins_env())
        } else if var("SEMAPHORE").is_ok() {
            Some(Self::get_semaphore_env())
        } else {
            Self::get_generic_env()
        }
    }

    pub fn from_ci(ci: CiService) -> Option<Self> {
        use CiService::*;
        match ci {
            Travis | TravisPro => {
                let mut temp = Self::get_travis_env();
                temp.name = ci;
                Some(temp)
            }
            Circle => Some(Self::get_circle_env()),
            Semaphore => Some(Self::get_semaphore_env()),
            Jenkins => Some(Self::get_jenkins_env()),
            _ => Self::get_generic_env(),
        }
    }

    /// Gets service variables from travis environment
    /// Warning is unable to figure out if travis pro or free so assumes free
    pub fn get_travis_env() -> Self {
        let id = var("TRAVIS_JOB_ID").ok();
        let pr = match var("TRAVIS_PULL_REQUEST") {
            Ok(ref s) if s != "false" => Some(s.to_string()),
            _ => None,
        };
        let branch = var("TRAVIS_BRANCH").ok();
        Service {
            name: CiService::Travis,
            job_id: id,
            number: None,
            build_url: None,
            pull_request: pr,
            branch: branch,
        }
    }

    pub fn get_circle_env() -> Self {
        let num = var("CIRCLE_BUILD_NUM").ok();
        let branch = var("CIRCLE_BRANCH").ok();
        Service {
            name: CiService::Circle,
            job_id: None, // Not happy with this but apparently it works
            number: num,
            build_url: None,
            pull_request: None,
            branch: branch,
        }
    }

    pub fn get_jenkins_env() -> Self {
        let num = var("BUILD_NUM").ok();
        let url = var("BUILD_URL").ok();
        let branch = var("GIT_BRANCH").ok();
        Service {
            name: CiService::Jenkins,
            job_id: None, // Not happy with this but apparently it works
            number: num,
            build_url: url,
            pull_request: None,
            branch: branch,
        }
    }

    pub fn get_semaphore_env() -> Self {
        let num = var("SEMAPHORE_BUILD_NUMBER").ok();
        let pr = var("PULL_REQUEST_NUMBER").ok();
        Service {
            name: CiService::Semaphore,
            job_id: None,
            number: num,
            pull_request: pr,
            branch: None,
            build_url: None,
        }
    }

    pub fn get_generic_env() -> Option<Self> {
        let name = var("CI_NAME").ok();
        let num = var("CI_BUILD_NUMBER").ok();
        let id = var("CI_JOB_ID").ok();
        let url = var("CI_BUILD_URL").ok();
        let branch = var("CI_BRANCH").ok();
        let pr = var("CI_PULL_REQUEST").ok();
        if name.is_some()
            || num.is_some()
            || id.is_some()
            || url.is_some()
            || branch.is_some()
            || pr.is_some()
        {
            let name = name.unwrap_or_else(|| "unknown".to_string());

            Some(Service {
                name: CiService::from_str(&name).unwrap(),
                job_id: id,
                number: num,
                pull_request: pr,
                branch: branch,
                build_url: url,
            })
        } else {
            None
        }
    }
}

/// Repo tokens are alternatives to Services and involve a secret token on coveralls
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum Identity {
    RepoToken(String),
    ServiceToken(String, Service),
}

impl Identity {
    /// Creates a report identity from a coveralls repo token if one is available
    /// Only checks via environment variables - this doesn't take into account
    /// the presence of a .coveralls.yml file
    pub fn from_token() -> Option<Self> {
        match var("COVERALLS_REPO_TOKEN") {
            Ok(token) => Some(Identity::RepoToken(token)),
            _ => None,
        }
    }

    /// Creates a report identity based on the CI service auto-detect functionality
    pub fn from_env() -> Option<Self> {
        let token = match var("COVERALLS_REPO_TOKEN") {
            Ok(token) => token,
            _ => String::new(),
        };
        match Service::from_env() {
            Some(s) => Some(Identity::ServiceToken(token, s)),
            _ => None,
        }
    }

    /// Prefers a coveralls repo token otherwise falls back on CI environment
    /// variables
    pub fn best_match() -> Option<Self> {
        if let Some(s) = Self::from_env() {
            Some(s)
        } else if let Some(s) = Self::from_token() {
            Some(s)
        } else {
            None
        }
    }

    pub fn best_match_with_token(token: String) -> Self {
        if let Some(Identity::ServiceToken(_, s)) = Self::from_env() {
            Identity::ServiceToken(token, s)
        } else {
            Identity::RepoToken(token)
        }
    }
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
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let size = 1 + match self.id {
            Identity::RepoToken(_) => 1 + self.commit.is_some() as usize,
            Identity::ServiceToken(_, _) => 2 + self.commit.is_some() as usize,
        };
        let mut s = serializer.serialize_struct("CoverallsReport", size)?;
        match self.id {
            Identity::RepoToken(ref r) => {
                s.serialize_field("repo_token", &r)?;
            }
            Identity::ServiceToken(ref r, ref serv) => {
                if !r.is_empty() {
                    s.serialize_field("repo_token", &r)?;
                }
                s.serialize_field("service_name", serv.name.value())?;
                if let Some(ref id) = serv.job_id {
                    s.serialize_field("service_job_id", id)?;
                }
                if let Some(ref num) = serv.number {
                    s.serialize_field("service_number", &num)?;
                }
                if let Some(ref url) = serv.build_url {
                    s.serialize_field("service_build_url", &url)?;
                }
                if let Some(ref branch) = serv.branch {
                    s.serialize_field("service_branch", &branch)?;
                }
                if let Some(ref pr) = serv.pull_request {
                    s.serialize_field("service_pull_request", &pr)?;
                }
            }
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

    use crate::*;
    use std::collections::HashMap;

    #[test]
    fn test_expand_lines() {
        let line_count = 10;
        let mut example: HashMap<usize, usize> = HashMap::new();
        example.insert(5, 1);
        example.insert(6, 1);
        example.insert(8, 2);

        let expected = vec![
            None,
            None,
            None,
            None,
            Some(1),
            Some(1),
            None,
            Some(2),
            None,
            None,
        ];

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
            line_number: 4,
            block_name: 1,
            branch_number: 2,
            hits: 0,
        };

        let v = vec![b1, b2];
        let actual = expand_branches(&v);
        let expected = vec![3, 1, 1, 1, 4, 1, 2, 0];
        assert_eq!(actual, expected);
    }
}
