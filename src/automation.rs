use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use soul_entity::cron::CronTask;
use std::path::{Path, PathBuf};

const STORE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Automation {
    pub name: String,
    pub schedule: String,
    pub goal: String,
    pub priority: u8,
    pub enabled: bool,
}

impl Automation {
    pub fn new(name: String, schedule: String, goal: String, priority: u8) -> Result<Self> {
        validate_name(&name)?;
        if goal.trim().is_empty() {
            bail!("automation goal cannot be empty");
        }
        if !(1..=10).contains(&priority) {
            bail!("priority must be between 1 and 10");
        }
        let schedule = normalize_schedule(&schedule)?;
        CronTask::new(&name, &schedule, &goal, priority)
            .map_err(|error| anyhow::anyhow!("invalid schedule: {error}"))?;
        Ok(Self {
            name,
            schedule,
            goal,
            priority,
            enabled: true,
        })
    }

    pub fn as_cron_task(&self) -> Result<CronTask> {
        CronTask::new(&self.name, &self.schedule, &self.goal, self.priority)
            .map_err(|error| anyhow::anyhow!("invalid stored automation '{}': {error}", self.name))
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AutomationStore {
    version: u32,
    automations: Vec<Automation>,
}

pub struct AutomationRepository {
    path: PathBuf,
}

impl AutomationRepository {
    pub fn in_config_dir(config_dir: &Path) -> Self {
        Self {
            path: config_dir.join("automations.json"),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn list(&self) -> Result<Vec<Automation>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let contents = std::fs::read_to_string(&self.path)
            .with_context(|| format!("cannot read {}", self.path.display()))?;
        let store: AutomationStore = serde_json::from_str(&contents)
            .with_context(|| format!("invalid automation store {}", self.path.display()))?;
        if store.version != STORE_VERSION {
            bail!(
                "unsupported automation store version {} in {}",
                store.version,
                self.path.display()
            );
        }
        for automation in &store.automations {
            validate_name(&automation.name)?;
            automation.as_cron_task()?;
        }
        Ok(store.automations)
    }

    pub fn add(&self, automation: Automation) -> Result<()> {
        let mut automations = self.list()?;
        if automations.iter().any(|item| item.name == automation.name) {
            bail!("automation '{}' already exists", automation.name);
        }
        automations.push(automation);
        self.save(automations)
    }

    pub fn remove(&self, name: &str) -> Result<bool> {
        let mut automations = self.list()?;
        let original_len = automations.len();
        automations.retain(|item| item.name != name);
        if automations.len() == original_len {
            return Ok(false);
        }
        self.save(automations)?;
        Ok(true)
    }

    pub fn set_enabled(&self, name: &str, enabled: bool) -> Result<bool> {
        let mut automations = self.list()?;
        let Some(automation) = automations.iter_mut().find(|item| item.name == name) else {
            return Ok(false);
        };
        automation.enabled = enabled;
        self.save(automations)?;
        Ok(true)
    }

    fn save(&self, automations: Vec<Automation>) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        let store = AutomationStore {
            version: STORE_VERSION,
            automations,
        };
        let bytes = serde_json::to_vec_pretty(&store)?;
        let temporary = self.path.with_extension("json.tmp");
        write_private(&temporary, &bytes)?;
        std::fs::rename(&temporary, &self.path)
            .with_context(|| format!("cannot replace {}", self.path.display()))?;
        Ok(())
    }
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        bail!("name must contain 1-64 ASCII letters, digits, '-' or '_'");
    }
    Ok(())
}

pub fn normalize_schedule(input: &str) -> Result<String> {
    let input = input.trim().to_ascii_lowercase();
    match input.as_str() {
        "hourly" | "chaque-heure" => return Ok("0 * * * *".into()),
        "daily" | "quotidien" => return Ok("0 0 * * *".into()),
        "weekly" | "hebdomadaire" => return Ok("0 0 * * 1".into()),
        _ => {}
    }
    if let Some(time) = input
        .strip_prefix("daily@")
        .or_else(|| input.strip_prefix("quotidien@"))
    {
        let (hour, minute) = parse_time(time)?;
        return Ok(format!("{minute} {hour} * * *"));
    }
    if let Some(minutes) = input
        .strip_prefix("every-")
        .and_then(|value| value.strip_suffix('m'))
        .or_else(|| {
            input
                .strip_prefix("toutes-les-")
                .and_then(|value| value.strip_suffix('m'))
        })
    {
        let minutes: u32 = minutes.parse().context("invalid minute interval")?;
        if minutes == 0 || minutes > 59 {
            bail!("minute interval must be between 1 and 59");
        }
        return Ok(format!("*/{minutes} * * * *"));
    }

    CronTask::new("validation", &input, "validation", 1)
        .map_err(|error| anyhow::anyhow!("invalid schedule: {error}"))?;
    Ok(input)
}

fn parse_time(input: &str) -> Result<(u32, u32)> {
    let (hour, minute) = input
        .split_once(':')
        .context("time must use the HH:MM format")?;
    let hour: u32 = hour.parse().context("invalid hour")?;
    let minute: u32 = minute.parse().context("invalid minute")?;
    if hour > 23 || minute > 59 {
        bail!("time must be between 00:00 and 23:59");
    }
    Ok((hour, minute))
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friendly_schedules_are_normalized() {
        assert_eq!(normalize_schedule("hourly").unwrap(), "0 * * * *");
        assert_eq!(normalize_schedule("daily@08:30").unwrap(), "30 8 * * *");
        assert_eq!(
            normalize_schedule("toutes-les-15m").unwrap(),
            "*/15 * * * *"
        );
    }

    #[test]
    fn repository_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let repository = AutomationRepository::in_config_dir(directory.path());
        repository
            .add(
                Automation::new(
                    "morning".into(),
                    "daily@08:30".into(),
                    "Prepare the daily brief".into(),
                    4,
                )
                .unwrap(),
            )
            .unwrap();
        let entries = repository.list().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].schedule, "30 8 * * *");
        assert!(repository.set_enabled("morning", false).unwrap());
        assert!(!repository.list().unwrap()[0].enabled);
        assert!(repository.remove("morning").unwrap());
        assert!(repository.list().unwrap().is_empty());
    }
}
