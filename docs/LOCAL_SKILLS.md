# Local Skills — Plugin System

## Overview

Local Skills allow extending Clawd with native Rust plugins compiled as shared
libraries (.so). Each skill is cryptographically signed and verified before loading.

## Architecture

```
~/.soulsystem/skills/
├── echo.so          # Plugin binary
├── echo.so.sig      # Signature (JSON)
├── myskill.so
└── myskill.so.sig
```

## Skill Signature Format (.sig)

```json
{
  "name": "echo",
  "sha256": "abc123def456...",
  "signature": [1, 2, 3, ...],
  "key_id": [4, 5, 6, ...]
}
```

Fields:
- `name`: matches the .so filename stem
- `sha256`: SHA-256 hash of the .so contents
- `signature`: HMAC-SHA256(hash || private_key)
- `key_id`: SHA-256(private_key) — public key identifier

## Lifecycle

```
1. Developer compiles Rust code → .so
2. Developer signs with: LocalSkillLoader::sign_skill()
   - Computes SHA-256(.so)
   - Signs with private key
   - Writes .so.sig
3. Operator loads: LocalSkillLoader::authorize(name, key_id, private_key)
4. discover() scans skills dir, verifies each .sig
5. Verified skills are loaded and available via /skill <name>
```

## Security Model

- **Signing**: HMAC-SHA256 using a secret key known only to the skill author
- **Verification**: requires registering the private key in `authorized_keys`
- **Integrity**: SHA-256 hash detects tampered .so files
- **Identity**: key_id (SHA-256 of private key) identifies which key signed the skill

## Builtin Skills

Builtin skills don't need .so files — they're compiled directly into Clawd:

| Name | Description |
|------|-------------|
| `echo` | Returns "ECHO: <input>" |

## API

### Creating a Skill

```rust
use soulsystem::local_skills::LocalSkill;

struct MySkill;
impl LocalSkill for MySkill {
    fn name(&self) -> &str { "myskill" }
    fn execute(&self, input: &str) -> Result<String> {
        Ok(format!("Processed: {}", input))
    }
}
```

### Signing a Compiled Skill

```rust
let private_key = b"my-32-byte-secret-key-for-skills";
LocalSkillLoader::sign_skill(
    Path::new("path/to/myskill.so"),
    private_key,
    "myskill",
)?;
```

### Loading Skills

```rust
let mut loader = LocalSkillLoader::new(
    LocalSkillLoader::default_skills_dir()
);
loader.authorize("myskill", key_id, private_key);
let count = loader.discover()?; // Loads and verifies all skills
```

## Commands

In Telegram via Clawd:

```
/skill echo hello world
> ECHO: hello world

/skill
> Skills disponibles: echo
> Usage: /skill <nom> <args>
```
