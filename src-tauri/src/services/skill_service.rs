use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use tokio::time::timeout;

use crate::models::{AppType, Skill, SkillMetadata, SkillRepo, SkillState};

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60);

pub struct SkillService {
    client: Client,
}

fn extract_skill_from_zip<R: Read + Seek>(
    mut archive: zip::ZipArchive<R>,
    target_dir: &Path,
    directory: &str,
) -> Result<()> {
    // 查找并解压技能目录（防止 Zip Slip）
    let mut found = false;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;

        let Some(entry_path) = file.enclosed_name().map(|p| p.to_owned()) else {
            continue;
        };

        // GitHub zip 结构通常是: {repo}-{branch}/{directory}/...
        let root = match entry_path.components().next() {
            Some(Component::Normal(s)) => s,
            _ => continue,
        };

        let expected_prefix = Path::new(root).join(directory);
        if !entry_path.starts_with(&expected_prefix) {
            continue;
        }

        found = true;

        let rel = entry_path
            .strip_prefix(&expected_prefix)
            .unwrap_or_else(|_| Path::new(""));

        if rel.as_os_str().is_empty() {
            continue;
        }

        // 二次校验：防止形如 "skill/../evil" 绕过 prefix 检查
        if rel.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            continue;
        }

        let output_path = target_dir.join(rel);

        if file.is_dir() {
            fs::create_dir_all(&output_path)?;
        } else {
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut output_file = fs::File::create(&output_path)?;
            std::io::copy(&mut file, &mut output_file)?;
        }
    }

    if !found {
        return Err(anyhow!("Skill directory not found in archive"));
    }

    Ok(())
}

impl SkillService {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .timeout(DOWNLOAD_TIMEOUT)
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client })
    }

    /// 获取技能安装目录
    fn get_skills_dir(app_type: &AppType) -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| anyhow!("Failed to get home directory"))?;

        let skills_dir = match app_type {
            AppType::Claude => home.join(".claude").join("skills"),
            AppType::Codex => home.join(".codex").join("skills"),
            AppType::Gemini => home.join(".gemini").join("skills"),
            AppType::ProxyCast => home.join(".proxycast").join("skills"),
        };

        Ok(skills_dir)
    }

    /// 列出所有技能
    pub async fn list_skills(
        &self,
        app_type: &AppType,
        repos: &[SkillRepo],
        installed_states: &HashMap<String, SkillState>,
    ) -> Result<Vec<Skill>> {
        let mut all_skills: HashMap<String, Skill> = HashMap::new();

        // 1. 从启用的仓库获取技能
        let enabled_repos: Vec<_> = repos.iter().filter(|r| r.enabled).collect();

        for repo in enabled_repos {
            match timeout(
                DOWNLOAD_TIMEOUT,
                self.fetch_skills_from_repo(repo, app_type, installed_states),
            )
            .await
            {
                Ok(Ok(skills)) => {
                    for skill in skills {
                        all_skills.insert(skill.key.clone(), skill);
                    }
                }
                Ok(Err(e)) => {
                    tracing::warn!(
                        "Failed to fetch skills from {}/{}: {}",
                        repo.owner,
                        repo.name,
                        e
                    );
                }
                Err(_) => {
                    tracing::warn!("Timeout fetching skills from {}/{}", repo.owner, repo.name);
                }
            }
        }

        // 2. 添加本地已安装但不在任何仓库中的技能
        let skills_dir = Self::get_skills_dir(app_type)?;
        if skills_dir.exists() {
            if let Ok(entries) = fs::read_dir(&skills_dir) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        let directory = entry.file_name().to_string_lossy().to_string();

                        // 检查是否已有相同 directory 的 skill（按 directory 去重）
                        let already_exists = all_skills.values().any(|s| s.directory == directory);

                        if !already_exists {
                            let key = format!("local:{}", directory);
                            let skill_md = entry.path().join("SKILL.md");
                            let (name, description) = if skill_md.exists() {
                                self.parse_skill_metadata(&skill_md)
                                    .map(|m| {
                                        (
                                            m.name.unwrap_or_else(|| directory.clone()),
                                            m.description.unwrap_or_default(),
                                        )
                                    })
                                    .unwrap_or_else(|_| (directory.clone(), String::new()))
                            } else {
                                (directory.clone(), String::new())
                            };

                            all_skills.insert(
                                key.clone(),
                                Skill {
                                    key,
                                    name,
                                    description,
                                    directory: directory.clone(),
                                    readme_url: None,
                                    installed: true,
                                    repo_owner: None,
                                    repo_name: None,
                                    repo_branch: None,
                                },
                            );
                        }
                    }
                }
            }
        }

        // 3. 排序并返回
        let mut skills: Vec<Skill> = all_skills.into_values().collect();
        skills.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(skills)
    }

    /// 从仓库获取技能列表
    async fn fetch_skills_from_repo(
        &self,
        repo: &SkillRepo,
        app_type: &AppType,
        installed_states: &HashMap<String, SkillState>,
    ) -> Result<Vec<Skill>> {
        let zip_url = format!(
            "https://github.com/{}/{}/archive/refs/heads/{}.zip",
            repo.owner, repo.name, repo.branch
        );

        // 下载 ZIP
        let response = self
            .client
            .get(&zip_url)
            .send()
            .await
            .context("Failed to download repository")?;

        if !response.status().is_success() {
            return Err(anyhow!("HTTP {}: {}", response.status(), zip_url));
        }

        let bytes = response.bytes().await.context("Failed to read response")?;

        // 解压并扫描
        let cursor = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor).context("Failed to open ZIP archive")?;

        let mut skills = Vec::new();
        let repo_key_prefix = format!("{}/{}:", repo.owner, repo.name);

        for i in 0..archive.len() {
            let mut file = archive.by_index(i).context("Failed to read ZIP entry")?;
            let file_path = file.name().to_string();

            if file_path.ends_with("/SKILL.md") || file_path.ends_with("\\SKILL.md") {
                let path = Path::new(&file_path);
                let directory = path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                // 读取并解析 SKILL.md
                let mut content = String::new();
                use std::io::Read;
                file.read_to_string(&mut content)
                    .context("Failed to read SKILL.md")?;

                let metadata = self.parse_skill_metadata_from_content(&content)?;
                let name = metadata.name.unwrap_or_else(|| directory.clone());
                let description = metadata.description.unwrap_or_default();

                let key = format!("{}{}", repo_key_prefix, directory);
                let app_key = format!("{}:{}", app_type.to_string().to_lowercase(), directory);
                let installed = installed_states.contains_key(&app_key);

                let readme_url = Some(format!(
                    "https://github.com/{}/{}/blob/{}/{}/SKILL.md",
                    repo.owner,
                    repo.name,
                    repo.branch,
                    path.parent().unwrap().to_str().unwrap_or("")
                ));

                skills.push(Skill {
                    key,
                    name,
                    description,
                    directory,
                    readme_url,
                    installed,
                    repo_owner: Some(repo.owner.clone()),
                    repo_name: Some(repo.name.clone()),
                    repo_branch: Some(repo.branch.clone()),
                });
            }
        }

        Ok(skills)
    }

    /// 安装技能
    pub async fn install_skill(
        &self,
        app_type: &AppType,
        repo_owner: &str,
        repo_name: &str,
        repo_branch: &str,
        directory: &str,
    ) -> Result<()> {
        let skills_dir = Self::get_skills_dir(app_type)?;
        fs::create_dir_all(&skills_dir).context("Failed to create skills directory")?;

        let target_dir = skills_dir.join(directory);
        if target_dir.exists() {
            fs::remove_dir_all(&target_dir).context("Failed to remove existing skill")?;
        }

        // 尝试多个分支
        let branches = if repo_branch == "main" {
            vec!["main", "master"]
        } else {
            vec![repo_branch]
        };

        let mut last_error = None;

        for branch in branches {
            let zip_url = format!(
                "https://github.com/{}/{}/archive/refs/heads/{}.zip",
                repo_owner, repo_name, branch
            );

            match self
                .download_and_extract(&zip_url, &target_dir, directory)
                .await
            {
                Ok(_) => return Ok(()),
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("Failed to install skill")))
    }

    /// 下载并解压技能
    async fn download_and_extract(
        &self,
        zip_url: &str,
        target_dir: &Path,
        directory: &str,
    ) -> Result<()> {
        let response = self
            .client
            .get(zip_url)
            .send()
            .await
            .context("Failed to download")?;

        if !response.status().is_success() {
            return Err(anyhow!("HTTP {}", response.status()));
        }

        let bytes = response.bytes().await.context("Failed to read response")?;
        let cursor = std::io::Cursor::new(bytes);
        let archive = zip::ZipArchive::new(cursor).context("Failed to open ZIP")?;
        extract_skill_from_zip(archive, target_dir, directory)
    }

    /// 卸载技能
    pub fn uninstall_skill(app_type: &AppType, directory: &str) -> Result<()> {
        let skills_dir = Self::get_skills_dir(app_type)?;
        let target_dir = skills_dir.join(directory);

        if target_dir.exists() {
            fs::remove_dir_all(&target_dir).context("Failed to remove skill directory")?;
        }

        Ok(())
    }

    /// 解析技能元数据
    fn parse_skill_metadata(&self, path: &Path) -> Result<SkillMetadata> {
        let content = fs::read_to_string(path).context("Failed to read SKILL.md")?;
        self.parse_skill_metadata_from_content(&content)
    }

    /// 从内容解析技能元数据
    fn parse_skill_metadata_from_content(&self, content: &str) -> Result<SkillMetadata> {
        let content = content.trim_start_matches('\u{feff}');
        let parts: Vec<&str> = content.splitn(3, "---").collect();

        if parts.len() < 3 {
            return Ok(SkillMetadata {
                name: None,
                description: None,
            });
        }

        let front_matter = parts[1].trim();
        let meta: SkillMetadata =
            serde_yaml::from_str(front_matter).context("Failed to parse YAML front matter")?;

        Ok(meta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_extract_skill_from_zip_blocks_zip_slip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let target_dir = tmp.path().join("target");
        std::fs::create_dir_all(&target_dir).expect("create target_dir");

        let cursor = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        let opts = zip::write::FileOptions::default();

        zip.start_file("repo-main/skill/ok.txt", opts)
            .expect("start ok.txt");
        zip.write_all(b"ok").expect("write ok");

        zip.start_file("repo-main/skill/../evil.txt", opts)
            .expect("start evil.txt");
        zip.write_all(b"pwned").expect("write evil");

        zip.start_file("repo-main/other/ignore.txt", opts)
            .expect("start ignore.txt");
        zip.write_all(b"ignore").expect("write ignore");

        let cursor = zip.finish().expect("finish zip");
        let bytes = cursor.into_inner();
        let archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("open zip");

        extract_skill_from_zip(archive, &target_dir, "skill").expect("extract");

        assert!(target_dir.join("ok.txt").exists());
        assert!(!tmp.path().join("evil.txt").exists());
        assert!(!target_dir.join("ignore.txt").exists());
    }

    #[test]
    fn test_extract_skill_from_zip_requires_directory_present() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let target_dir = tmp.path().join("target");
        std::fs::create_dir_all(&target_dir).expect("create target_dir");

        let cursor = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        let opts = zip::write::FileOptions::default();

        zip.start_file("repo-main/other/file.txt", opts)
            .expect("start file.txt");
        zip.write_all(b"x").expect("write x");

        let cursor = zip.finish().expect("finish zip");
        let bytes = cursor.into_inner();
        let archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("open zip");

        let err = extract_skill_from_zip(archive, &target_dir, "skill").unwrap_err();
        assert!(err.to_string().contains("Skill directory not found"));
    }
}
