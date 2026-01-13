# Walsync Deployment Checklist

Complete checklist for taking Walsync from development to production.

## ✅ Development Phase (COMPLETE)

- [x] Implement SHA256 checksums in S3 metadata
- [x] Add multi-database support tests
- [x] Create comprehensive test suite (32 tests)
- [x] Verify byte-for-byte data integrity
- [x] Complete Python bindings (PyO3)
- [x] Set up maturin for wheel building
- [x] Create professional documentation site
- [x] Design logo and brand assets
- [x] Set up GitHub Actions workflows

## ⬜ Pre-Release Phase (READY)

### 1. Verify Everything Works Locally

```bash
# Run all tests
./run_tests.sh

# Build CLI
cargo build --release

# Build Python package
cd docs && npm install && npm run build

# Verify docs build
# Visit http://localhost:3000
```

- [ ] All tests pass
- [ ] CLI builds successfully
- [ ] Docs build without errors
- [ ] No warnings or deprecations

### 2. Final Documentation Review

- [ ] README.md is accurate and complete
- [ ] RELEASE.md reflects current process
- [ ] CODE_OF_CONDUCT.md exists (if needed)
- [ ] LICENSE file is correct (Apache 2.0)
- [ ] All docs pages have correct links

### 3. GitHub Configuration

- [ ] Repository is public
- [ ] Repository description is set
- [ ] Topics are added: sqlite, wal, s3, backup, database
- [ ] GitHub Pages settings point to `/docs` or automatic
- [ ] Branch protection is configured (if needed)

### 4. Crates.io Setup

- [ ] Logged in to Crates.io: `cargo login`
- [ ] Verified Cargo.toml metadata
- [ ] Checked for name conflicts on crates.io

### 5. PyPI Trusted Publisher Setup

Go to **GitHub Repository Settings**:

1. Settings → Environments → New environment
2. Create environment named: `pypi`
3. Add deployment branches (allow `main`)
4. Add trusted publisher:
   - Publisher type: GitHub Actions
   - Repository: `russellromney/personal-website`
   - Repository owner: `russellromney`
   - Workflow: `pypi-release.yml`
   - Environment: `pypi`

- [ ] PyPI Trusted Publisher configured
- [ ] Environment has appropriate permissions

## ⬜ Release Phase (FOLLOW THESE STEPS)

### 1. Version Bump

```bash
# Update versions in both files
sed -i '' 's/version = "0.2.0"/version = "0.3.0"/' Cargo.toml pyproject.toml

# Verify
grep version Cargo.toml pyproject.toml | head -4

# Commit
git add Cargo.toml pyproject.toml
git commit -m "chore: bump version to 0.3.0"
```

### 2. Create Release Tag

```bash
# Tag the release (GitHub Actions triggers on this)
git tag v0.3.0

# Push tag (triggers PyPI release)
git push --tags

# Monitor GitHub Actions
# Visit: https://github.com/russellromney/walsync/actions
```

### 3. Publish to Crates.io

```bash
# Dry run first
cargo publish --dry-run

# Actual publish
cargo publish

# Verify on crates.io
# https://crates.io/crates/walsync
```

### 4. Verify Releases

- [ ] GitHub Actions build succeeds (check Actions tab)
- [ ] PyPI release published: https://pypi.org/project/walsync/
- [ ] Crates.io release published: https://crates.io/crates/walsync
- [ ] Docs deployed: https://russellromney.github.io/walsync/

### 5. Test Installations

```bash
# Test CLI installation
cargo install walsync --version 0.3.0
walsync --version

# Test Python installation
pip install walsync --upgrade
python -c "from walsync import WalSync; print('OK')"
```

- [ ] CLI installs and runs
- [ ] Python package installs and imports
- [ ] Both work with sample operations

### 6. Create GitHub Release

```bash
# Create release on GitHub with release notes
gh release create v0.3.0 --generate-notes
```

Or do manually:

1. Go to GitHub Releases
2. Click "Draft a new release"
3. Select tag `v0.3.0`
4. Add release notes (copy from CHANGELOG)
5. Publish release

- [ ] GitHub Release created with notes

### 7. Announce Release

- [ ] Post on Hacker News (if significant)
- [ ] Update README if needed
- [ ] Share on social media (if applicable)
- [ ] Send to relevant communities

## ⬜ Post-Release Phase

### 1. Monitor for Issues

```bash
# Watch for issue reports
# Check PyPI: https://pypi.org/project/walsync/
# Check Crates.io: https://crates.io/crates/walsync
# Monitor GitHub Issues
```

- [ ] No critical issues reported
- [ ] Users can install successfully
- [ ] Documentation is accessible

### 2. Update Resources

- [ ] Update website/blog with announcement
- [ ] Add release to documentation changelog
- [ ] Update any external references

## 🎯 Release Schedule

**Recommended timeline:**
- **Patch releases** (0.2.1, 0.2.2): Weekly or as-needed for bug fixes
- **Minor releases** (0.3.0, 0.4.0): Monthly with new features
- **Major releases** (1.0.0): When API stabilizes or breaking changes needed

## 🚀 First Release (v0.2.0)

This will be the first public release. Extra care needed:

1. ✅ All tests passing
2. ✅ Documentation complete
3. ✅ Python bindings working
4. ✅ CLI functional
5. ✅ Logo and branding ready

### Current Status: READY FOR RELEASE

All items are complete. Ready to proceed with:
```bash
git tag v0.2.0
git push --tags
```

## Troubleshooting

### PyPI Upload Fails

- [ ] Check Trusted Publisher is configured
- [ ] Check tag format: `v0.X.Y` (must start with `v`)
- [ ] Check GitHub Actions logs for wheel build errors
- [ ] Ensure environment name matches: `pypi`

### Crates.io Upload Fails

- [ ] Check logged in: `cargo login`
- [ ] Check Cargo.toml has all required fields
- [ ] Check for name conflicts: https://crates.io/crates/walsync
- [ ] Run `cargo publish --dry-run` first

### Docs Not Publishing

- [ ] Check GitHub Pages settings
- [ ] Verify branch is set correctly
- [ ] Check workflow file: `.github/workflows/deploy-docs.yml`
- [ ] Check for build errors in Actions tab

## References

- [Crates.io Publishing Guide](https://doc.rust-lang.org/cargo/publishing/)
- [PyPI Documentation](https://packaging.python.org/)
- [Maturin Documentation](https://www.maturin.rs/)
- [GitHub Pages Guide](https://docs.github.com/en/pages)
- [Astro Deployment Guide](https://docs.astro.build/guides/deploy/)

---

**Version:** 0.2.0
**Status:** ✅ Ready for Release
**Date:** January 12, 2026
