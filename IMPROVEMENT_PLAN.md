# just-ai Improvement Plan

## Current Project State Analysis

| Feature | CLI | MCP Server | Tauri GUI | VS Code Extension |
|---------|-----|------------|-----------|-------------------|
| Suggest | ✅ | ✅ | ✅ | ✅ |
| Explain | ✅ | ✅ | ✅ | ✅ |
| Add Recipe | ✅ | ✅ | ✅ | ✅ |
| Fix Recipe | ✅ | ✅ | ✅ | ✅ |
| Workflow (multi-recipe) | ✅ | ✅ | ✅ | ✅ |
| Fix Batch | ✅ | ✅ | ✅ | ✅ |
| Explain Batch | ✅ | ✅ | ✅ | ✅ |
| Migrate Analyze | ✅ | ✅ | ✅ | ✅ |
| Migrate Modularize | ✅ | ✅ | ✅ | ✅ |
| Migrate Deduplicate | ✅ | ✅ | ✅ | ✅ |
| Template Create | ✅ | ✅ | ✅ | ❌ |
| Instantiate Template | ✅ | ✅ | ✅ | ❌ |
| Compose Workflow | ✅ | ✅ | ✅ | ❌ |
| Doctor | ✅ | ✅ | ❌ (via execute_run) | ✅ |
| Export Context | ✅ | ❌ | ❌ | ✅ |
| Run Recipe | ✅ | ✅ | ✅ | ✅ |
| History | ✅ | ✅ | ✅ | ✅ |
| Config Validate/Schema | ✅ | ❌ | ❌ | ❌ |

---

## Phase 1: VS Code Extension Parity (High Priority)

### 1.1 Add Missing AI Commands to VS Code
- [ ] `just-ai.template` command with input dialog for request
- [ ] `just-ai.instantiateTemplate` command with template name, values input, write option
- [ ] `just-ai.composeWorkflow` command with request input, write option
- [ ] Add client methods: `template()`, `instantiateTemplate()`, `composeWorkflow()`

### 1.2 Add Missing Utility Commands
- [ ] `just-ai.exportContext` (already in client, missing command registration)
- [ ] `just-ai.runRecipe` improvement: show recipe picker from history/context

---

## Phase 2: Tauri GUI Enhancements (High Priority)

### 2.1 UI Components for New Commands
- [ ] **Template Panel**: Create template form → show generated template → instantiate button
- [ ] **Instantiate Template Dialog**: Template dropdown (fetched from justfile) → parameter inputs → preview/write
- [ ] **Compose Workflow Panel**: Similar to workflow but shows source (existing/new/modified)

### 2.2 Missing Features
- [ ] **Doctor Panel**: Visual risk dashboard (currently only via execute_run)
- [ ] **Export Context Button**: Download JSON context for AI tools
- [ ] **Config Validation**: UI for just-ai.toml validation
- [ ] **Interactive Migrate Deduplicate**: Step-through merge conflicts

### 2.3 UI/UX Improvements
- [ ] Dark/light theme support (follow VS Code theme)
- [ ] Keyboard shortcuts for common actions
- [ ] Persistent view state (remember panel sizes, tabs)
- [ ] Better loading states with progress indicators
- [ ] Toast notifications for success/error

---

## Phase 3: Core Functionality Enhancements (Medium Priority)

### 3.1 Template Storage & Management
- [ ] **Template Persistence**: Store templates in `.just/templates/` directory
- [ ] **Template Registry**: List, search, edit, delete templates
- [ ] **Template Categories**: Build, Test, Deploy, Lint, Security, etc.
- [ ] **Template Versioning**: Track template changes
- [ ] **Built-in Templates**: Ship with common templates (CI/CD, test matrix, release, etc.)

### 3.2 Advanced Workflow Features
- [ ] **Workflow Templates**: Pre-defined multi-recipe patterns
- [ ] **Conditional Recipes**: If/else logic in workflow definitions
- [ ] **Parallel Execution**: Run independent recipes concurrently
- [ ] **Workflow Variables**: Pass data between recipes
- [ ] **Workflow Visualization**: Graph view of dependencies

### 3.3 Migrate Improvements
- [ ] **Custom Grouping Rules**: Regex/named patterns for modularize
- [ ] **Preserve Comments**: Keep documentation during refactoring
- [ ] **Import Aliases**: Support `import 'utils.just' as u`
- [ ] **Cross-file Dependencies**: Track deps across modules
- [ ] **Undo/Redo**: Transaction log for migrate operations

---

## Phase 4: AI Provider & Prompt Engineering (Medium Priority)

### 4.1 Provider Enhancements
- [ ] **Local LLM Support**: Ollama, llama.cpp, LM Studio integration
- [ ] **Provider Fallback**: Auto-switch on rate limits/errors
- [ ] **Cost Tracking**: Token usage per command
- [ ] **Model Selection**: Per-command model config (fast vs smart)

### 4.2 Prompt Improvements
- [ ] **Few-shot Examples**: Better prompt templates with examples
- [ ] **Context Pruning**: Smart context selection for large projects
- [ ] **Custom Prompts**: User-defined prompt templates
- [ ] **Prompt Versioning**: Track prompt changes for reproducibility

### 4.3 Structured Output
- [ ] **Streaming Responses**: Real-time token streaming in GUI
- [ ] **Partial Results**: Show incremental output for long operations
- [ ] **Confidence Scores**: AI confidence in proposals

---

## Phase 5: Project Intelligence (Low Priority)

### 5.1 Static Analysis
- [ ] **Unused Recipe Detection**: Find dead code
- [ ] **Circular Dependency Detection**: Full graph analysis
- [ ] **Recipe Complexity Metrics**: Lines, deps, params
- [ ] **Security Scanning**: Dangerous commands (rm -rf, curl | sh, etc.)

### 5.2 History & Analytics
- [ ] **Run Analytics Dashboard**: Success rate, duration trends
- [ ] **Recipe Heatmap**: Most/least used recipes
- [ ] **Failure Pattern Recognition**: Auto-group similar failures
- [ ] **Performance Regression Detection**: Compare run times

### 5.3 Cross-repo Intelligence
- [ ] **Template Sharing**: Import templates from other projects
- [ ] **Common Patterns**: Detect patterns across workspaces
- [ ] **Team Templates**: Organization-level template registry

---

## Phase 6: Developer Experience (Low Priority)

### 6.1 CLI Enhancements
- [ ] **Shell Completions**: Generate for zsh, fish, powershell
- [ ] **Interactive Mode**: TUI for common operations
- [ ] **Watch Mode**: Auto-run on justfile changes
- [ ] **Parallel Execution**: `--jobs N` for batch operations

### 6.2 Testing & Quality
- [ ] **Integration Tests**: End-to-end CLI + GUI + MCP
- [ ] **Contract Tests**: AI response schema validation
- [ ] **Property-based Tests**: Recipe generation invariants
- [ ] **Benchmark Suite**: Performance regression tracking

### 6.3 Documentation
- [ ] **API Documentation**: Rustdoc for public APIs
- [ ] **Migration Guide**: Version upgrade instructions
- [ ] **Video Tutorials**: Common workflows
- [ ] **Architecture Decision Records**: ADR log

---

## Phase 7: Enterprise Features (Future)

### 7.1 Team & Organization
- [ ] **SSO Integration**: GitHub, GitLab, Okta
- [ ] **Role-based Access**: Admin, Developer, Viewer
- [ ] **Audit Log**: All AI actions with timestamps
- [ ] **Policy Engine**: Enforce recipe standards

### 7.2 CI/CD Integration
- [ ] **GitHub Actions**: just-ai workflow steps
- [ ] **GitLab CI**: Native integration
- [ ] **PR Reviews**: Auto-suggest recipes on PR
- [ ] **Dependency Updates**: Auto-generate update recipes

---

## Implementation Priority Matrix

| Phase | Effort | Impact | Dependencies |
|-------|--------|--------|--------------|
| 1 - VS Code Parity | Low | High | None |
| 2 - Tauri UI | Medium | High | Phase 1 |
| 3 - Core Features | High | High | Phase 2 |
| 4 - AI Providers | Medium | Medium | None |
| 5 - Intelligence | High | Medium | Phase 3 |
| 6 - DX | Medium | Medium | None |
| 7 - Enterprise | Very High | Low | All above |

---

## Quick Wins (Can start immediately)

1. **VS Code Template Commands** - 2-3 hours
2. **Tauri Export Context Button** - 1 hour
3. **Tauri Config Validation** - 2 hours
4. **CLI Shell Completions** - 2 hours
5. **Built-in Templates** - 4 hours

---

## Recommended Next Steps

### Sprint 1 (Week 1-2): VS Code Parity
- Add template, instantiate-template, compose-workflow commands
- Add export-context command
- Test all commands end-to-end

### Sprint 2 (Week 3-4): Tauri GUI Polish
- Implement Template/Compose panels
- Add Doctor visual dashboard
- Theme support and keyboard shortcuts

### Sprint 3 (Week 5-6): Core Features
- Template persistence system
- Built-in template library
- Advanced migrate options

### Sprint 4 (Week 7-8): AI & DX
- Local LLM provider support
- Cost tracking
- Shell completions
- Documentation

---

## Technical Debt to Address

1. **Error Handling**: Standardize error types across crates
2. **Testing**: Increase coverage for proposal handling
3. **Performance**: Cache project context between commands
4. **Async**: Full async/await in Tauri commands
5. **Logging**: Structured logging with tracing

---

## Notes

- All phases maintain backward compatibility
- Each phase delivers user-visible value
- MCP Server is feature-complete; focus on consumers
- Consider extracting common AI logic to shared crate
- Monitor AI provider API changes (OpenAI, Anthropic, etc.)