# GitHub Actions Workflows

This directory contains automated workflows for ClippyB.

## Workflows

### 1. Continuous Integration (CI) - `ci.yml`
Runs on every push and pull request to ensure code quality:
- Runs on Windows, Linux, and macOS
- Checks code formatting with `cargo fmt`
- Runs linting with `cargo clippy`
- Runs all tests
- Builds the project

### 2. Release - `release.yml`
Automatically creates releases when you push a version tag:
- Triggers on tags matching `v*.*.*` pattern (e.g., `v1.0.0`)
- Generates changelog from commit messages
- Builds executables for all platforms:
  - Windows (x86_64)
  - Linux (x86_64)
  - macOS (Intel and Apple Silicon)
- Creates a GitHub release with:
  - Automatic changelog from commits
  - Platform-specific executables
  - Installation instructions

## How to Create a Release

1. Commit all your changes
2. Create and push a version tag:
   ```bash
   git tag v1.0.0
   git push origin v1.0.0
   ```
3. GitHub Actions will automatically:
   - Create a new release
   - Generate changelog from commits since last tag
   - Build and upload executables for all platforms

## Commit Message Format

For better changelogs, follow conventional commit format:
- `feat: Add new feature`
- `fix: Fix bug in X`
- `docs: Update documentation`
- `chore: Update dependencies`
- `refactor: Refactor Y module`

The changelog will automatically extract these messages and format them nicely in the release notes.