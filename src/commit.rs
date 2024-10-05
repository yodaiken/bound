use std::ffi::OsStr;
use std::io;
use std::io::{BufRead, BufReader, Read};
use std::iter::Peekable;
use std::path::PathBuf;
use std::process::{ChildStdout, Command, Stdio};

pub struct LineReader<R> {
    reader: BufReader<R>,
}

impl<R: Read> LineReader<R> {
    fn new(reader: R) -> Self {
        LineReader {
            reader: BufReader::new(reader),
        }
    }
}

impl<R: Read> Iterator for LineReader<R> {
    type Item = io::Result<String>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => None,
            Ok(_) => {
                // println!("Debug: {}", line);
                Some(Ok(line.trim_end().to_string()))
            },
            Err(e) => Some(Err(e)),
        }
    }
}

pub struct CommitIterator<R: Read> {
    lines: Peekable<LineReader<R>>,
}

impl<R: Read> CommitIterator<R> {
    fn new(lines: Peekable<LineReader<R>>) -> Self {
        CommitIterator { lines }
    }
}

impl<R: Read> Iterator for CommitIterator<R> {
    type Item = io::Result<CommitInfo>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut commit_info = CommitInfo {
            id: String::new(),
            timestamp: 0,
            author_name: String::new(),
            author_email: String::new(),
            file_changes: Vec::new(),
        };

        // Parse commit header and check for EOF
        if let Some(Ok(line)) = self.lines.next() {
            if line != "COMMIT" {
                return Some(Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Expected COMMIT",
                )));
            }
        } else {
            return None;
        }

        // Parse commit details
        for _ in 0..4 {
            if let Some(Ok(line)) = self.lines.next() {
                match commit_info.id.is_empty() {
                    true => commit_info.id = line,
                    false => match commit_info.timestamp {
                        0 => {
                            commit_info.timestamp = match line.parse() {
                                Ok(timestamp) => timestamp,
                                Err(e) => {
                                    return Some(Err(io::Error::new(io::ErrorKind::InvalidData, e)))
                                }
                            };
                        }
                        _ => match commit_info.author_name.is_empty() {
                            true => commit_info.author_name = line,
                            false => commit_info.author_email = line,
                        },
                    },
                }
            } else {
                return Some(Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Incomplete commit info",
                )));
            }
        }

        // Expect an empty line or EOF, skip it if it's there
        match self.lines.next() {
            Some(Ok(line)) if line.trim().is_empty() => {},
            Some(Ok(line)) => return Some(Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Expected empty line, got: '{}'", line),
            ))),
            Some(Err(e)) => return Some(Err(e)),
            None => {},
        }

        // Parse file changes
        while let Some(Ok(line)) = self.lines.peek() {
            if line == "COMMIT" {
                break;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() == 3 {
                commit_info.file_changes.push(FileChange {
                    insertions: parts[0].parse().unwrap_or(0),
                    deletions: parts[1].parse().unwrap_or(0),
                    path: parts[2].to_string(),
                });
            } else {
                return Some(Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid file change format: '{}'", line),
                )));
            }
            self.lines.next(); // Consume the peeked line
        }

        Some(Ok(commit_info))
    }
}

pub struct CommitInfo {
    pub id: String,
    pub timestamp: i64,
    pub author_name: String,
    pub author_email: String,
    pub file_changes: Vec<FileChange>,
}

pub struct FileChange {
    pub insertions: i32,
    pub deletions: i32,
    pub path: String,
}

fn execute_git<I, S>(args: I, cwd: &PathBuf) -> Result<LineReader<ChildStdout>, io::Error>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .spawn()?
        .stdout
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Could not capture stdout"))?;

    Ok(LineReader::new(output))
}

fn parse_commit<R: Read>(lines: LineReader<R>) -> CommitIterator<R> {
    CommitIterator::new(lines.peekable())
}

pub fn git_log_commits(
    since: &str,
    until: &str,
    cwd: &PathBuf,
) -> Result<impl Iterator<Item = Result<CommitInfo, io::Error>>, io::Error> {
    execute_git(
        [
            "log",
            "--no-merges",
            "--format=COMMIT%n%H%n%at%n%an%n%ae",
            "--numstat",
            &format!("--since={}", since),
            &format!("--until={}", until),
        ],
        cwd,
    )
    .map(parse_commit)
}

pub fn read_file_at_commit(
    commit_id: &str,
    file_path: &str,
    cwd: &PathBuf,
) -> Result<Option<String>, io::Error> {
    let output = Command::new("git")
        .args(["show", &format!("{}:{}", commit_id, file_path)])
        .current_dir(cwd)
        .output()?;

    if output.status.success() {
        let content = String::from_utf8(output.stdout)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Some(content))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.starts_with("fatal: path") {
            Ok(None)
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::Other, stderr))
        }
    }
}
