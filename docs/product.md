# Product: Eimer

## Vision
Personal developer toolkit - a collection of CLI tools that automate Azure DevOps and GitHub workflows (PRs, pipelines, releases, tasks) from the terminal.

## Target Users
Moritz Froelich (solo developer) - used daily to manage work items, PRs, CI/CD pipelines, and release changelogs without leaving the terminal. Potentially shareable with teammates on the same Azure DevOps org.

## Goals
- Eliminate context-switching between terminal and Azure DevOps/GitHub web UI
- Streamline release changelog generation and distribution (Teams webhooks)
- Unify all dev workflow CLIs under one `eimer` meta-command
- Keep tools fast, zero-config-by-default, with optional persistent config
- Publish release CLI to Azure Artifacts as `@tapio/release` for company-wide use via `bunx`

## Core Features
1. **PR Management** - Create PRs, view comments, copy references, check merge readiness (Azure DevOps + GitHub)
2. **Pipeline Operations** - List/trigger/watch/open Azure DevOps pipeline runs
3. **Release Workflows** - Generate grouped changelogs from pipeline commits, approve pending deployments, post to Teams
4. **Task Management** - Create/list/start/close Azure DevOps work items in active sprint
5. **Unified Meta-CLI** - `eimer` wraps all sub-CLIs with shared config (`~/.config/eimer/config.json`)
6. **Helpers** - Ad-hoc utility scripts (music collection/tagging, i18n tools)
