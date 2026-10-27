# Developer verification

Use the prebuilt native Debian 13 quality image through the maintained project
container launcher. Run `make lint static-analysis` before a push. Run affected
tests while iterating; record evidence under ignored `.work/`.

Pull requests require `make quality platform-verify coverage`: documentation and
static checks once, native amd64/arm64 tests and packages, and 100% production
line/branch coverage on amd64 only. Test code is not production coverage.

Workflow implementations live in `rptadv-shared-library-workflows`. Ordinary
pushes run fast checks; only a validated main revision can be released. Debian
12 packages and live node changes require separate explicit authorization.
