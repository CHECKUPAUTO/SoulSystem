# AI-Assisted Development Guidelines

**Source:** OpenEvolve Night Cycle Analysis (2026-04-11)  
**Scope:** OpenClaw AI/Human Collaboration Patterns

---

## Overview

This document captures the emerging patterns for AI-assisted development observed in the OpenClaw codebase, particularly the use of Claude Opus 4.6 (1M context) for security analysis.

## Observed Patterns

### 1. Security Co-Authorship

**Commit Pattern:** `Co-authored-by: Claude Opus 4.6 <claude@anthropic.com>`

**Example:**
- Commit 13dfd633: Block dangerous config mutations
- Lines changed: +1,315 lines (+832 lines of tests)
- Human author: Agustin Rivera
- AI contribution: Security guard implementation, test generation

### Benefits
- Large context analysis (>800k tokens)
- Comprehensive test coverage generation
- Security vulnerability identification
- Pattern recognition across multiple files

---

## Guidelines for AI Collaboration

### When to Use AI Assistance

| Scenario | AI Suitability | Human Role |
|----------|---------------|------------|
| Security audits | High | Review & approve |
| Test generation | High | Define test strategy |
| Documentation | Medium | Review & edit |
| Refactoring | Medium | Define scope |
| Architecture design | Low-Medium | Lead design |
| Bug fixes | Medium | Provide context |
| Feature implementation | Low | Lead implementation |

### Effective Prompting Patterns

#### Security Audit Prompt

```
Review the following gateway tool implementation for security 
vulnerabilities. Focus on:
1. Remote configuration mutation risks
2. Privilege escalation vectors
3. Input validation gaps
4. Authentication/authorization bypasses

Context: [paste relevant code]

Please provide:
1. Vulnerability analysis
2. Recommended guards/fixes
3. Test cases for each vulnerability
```

#### Test Generation Prompt

```
Given this security guard implementation, generate comprehensive 
test cases covering:
1. Happy path - valid mutations allowed
2. Security violations - dangerous mutations blocked
3. Edge cases - malformed inputs
4. Boundary conditions - at the limit

Implementation: [paste code]

Output format: Jest test cases with TypeScript types
```

---

## Quality Assurance

### Human Review Requirements

AI-generated code MUST be reviewed for:

1. **Correctness**: Does the code actually solve the problem?
2. **Security**: Are there subtle vulnerabilities introduced?
3. **Maintainability**: Is the code readable and maintainable?
4. **Performance**: Are there performance implications?
5. **Style**: Does it match project conventions?

### Review Checklist

- [ ] AI contribution is clearly marked (co-author attribution)
- [ ] Changes are minimal and focused
- [ ] Tests pass (not just written)
- [ ] No hardcoded secrets or credentials
- [ ] Error handling is appropriate
- [ ] Documentation is accurate

---

## Documentation Requirements

### Commit Message Format

```
<type>: <description>

[optional body explaining AI contribution]

Co-authored-by: Claude <claude@anthropic.com>
```

### PR Description Template

```markdown
## Summary
Brief description of changes

## AI Assistance
- [ ] No AI assistance
- [ ] AI-assisted (specify model and role)
  - Model: Claude Opus 4.6
  - Role: Security analysis, test generation
  - Human review: [Reviewer name]

## Checklist
- [ ] Code reviewed by human
- [ ] Tests pass
- [ ] Security implications considered
- [ ] Documentation updated
```

---

## Best Practices

### 1. Scope Definition

Define clear scope before engaging AI:
- Specific security boundaries to check
- Files/modules to analyze
- Expected output format

### 2. Context Management

- Provide complete context to AI
- Include related files, not just target file
- Document assumptions and constraints

### 3. Iterative Refinement

- Review AI output critically
- Ask follow-up questions
- Iterate on implementation

### 4. Knowledge Capture

- Document AI-discovered patterns
- Update security guidelines based on findings
- Share insights with team

---

## Risk Mitigation

### Potential Risks

| Risk | Mitigation |
|------|------------|
| Hallucinated vulnerabilities | Cross-check with security team |
| Over-engineered solutions | Keep changes minimal |
| Test coverage gaps | Manual review of edge cases |
| Context misunderstanding | Provide clear, complete context |

### Red Flags

- AI suggests architectural changes without understanding constraints
- Generated code doesn't compile
- Tests are tautological (test the mock, not the code)
- Security "fixes" that disable functionality

---

## Metrics to Track

### AI Contribution Metrics

- % of commits with AI co-authorship
- Lines of AI-generated code vs human-written
- Bug rate in AI-generated code vs human
- Time to review AI-generated PRs

### Quality Metrics

- Test coverage of AI-generated code
- Security issues found in AI code
- Time to fix AI-generated bugs
- Developer satisfaction with AI assistance

---

## Future Considerations

### Emerging Patterns

1. **Hybrid development**: AI generates, human reviews
2. **AI-first security**: AI audits before human review
3. **Test generation**: AI writes tests, human defines strategy
4. **Documentation**: AI drafts, human edits

### Integration with CI/CD

Consider:
- AI pre-flight checks for security
- Automated test generation on PR
- AI-generated PR summaries
- Smart reviewer assignment based on AI contributions

---

## References

- Commit 13dfd633: Claude Opus 4.6 co-authored security hardening
- OpenClaw contribution guidelines
- AI-assisted PR policy

---

*Generated by OpenEvolve Auto-Apply*  
*Timestamp: 2026-04-11T04:27:00Z*
