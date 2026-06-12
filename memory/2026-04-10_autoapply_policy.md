# Policy Change: Auto-Apply OpenEvolve Suggestions

**Date:** 2026-04-10 23:52 UTC
**Decision:** Automatic application of OpenEvolve night cycle suggestions

---

## Policy

**Previous:** Suggestions presented to user for manual approval before application.

**New:** Suggestions applied automatically without user confirmation.

**Scope:**
- All OpenEvolve night cycle reports
- All IronReview analysis outputs
- All evolved skills improvements
- Code fixes, refactoring, documentation updates

**Exceptions (still require approval):**
- Changes to OpenClaw core gateway/protocol
- Security-sensitive modifications (auth, tokens, credentials)
- Breaking changes to existing APIs
- Deletions of user data or configuration

---

## Automation

### Immediate Actions
- [x] Apply current batch of suggestions (exec-evolved, read-evolved, edit-evolved)

### Future Automation
- [ ] Cron job: Check for new reports every 15 minutes
- [ ] Auto-apply safe changes (skills, documentation, non-breaking)
- [ ] Log all changes to `evolution/auto_apply_log.json`
- [ ] Report summary to user after batch application

---

## Safety Measures

1. **Backup before edit**: All files backed up with timestamp
2. **Rollback capability**: Revert commands documented
3. **Audit trail**: All changes logged
4. **Dry-run first**: Complex changes previewed before apply

---

*Decision by: Human*
*Documented by: SoulLink V12*
