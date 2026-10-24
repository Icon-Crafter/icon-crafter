"use strict";

// Commit message gate (commitlint + husky).
//
// The release automation reads conventional-commit prefixes to decide what
// ships (see .github/workflows/auto-ship.yml), so ambiguous messages are
// rejected locally instead of failing CI later.
module.exports = {
  extends: [],
  rules: {
    "type-enum": [2, "always", [
      "feat", "fix", "perf", "refactor", "docs", "test", "build", "ci",
      "chore", "revert", "style",
    ]],
    "scope-empty": [1, "never"],
    "type-case": [2, "always", "lower"],
    "subject-empty": [2, "never"],
    "header-max-length": [2, "always", 100],
  },
};
