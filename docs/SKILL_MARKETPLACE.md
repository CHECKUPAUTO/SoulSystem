# Skill Marketplace

## Format

Chaque skill est un fichier YAML :

```yaml
name: my_skill
version: "1.0.0"
description: "Description du skill"
entrypoint: "libmy_skill.so"
```

## Installation

```bash
soulsystem skill install https://example.com/skills/my_skill.tar.gz
```

## Développement

Implémenter le trait `Skill` :

```rust
pub trait Skill {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn execute(&self, input: &str) -> String;
}
```
