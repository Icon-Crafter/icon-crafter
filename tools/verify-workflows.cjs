// Workflow YAML syntax verification.
//
// GitHub Actions does not provide a local validator, so this script parses
// every file under .github/workflows (and dependabot.yml) with a real YAML
// parser and asserts the structure the workflows rely on. It cannot prove that
// a runner will accept the definitions, but it catches the common failures:
// bad indentation, tabs, duplicate keys, wrong trigger shape.
"use strict";

const fs = require("node:fs");
const path = require("node:path");
const YAML = (() => {
  try {
    return require("yaml");
  } catch {
    return null;
  }
})();

if (!YAML) {
  console.error("the 'yaml' package is required (pnpm add -D yaml)");
  process.exit(2);
}

const root = path.resolve(__dirname, "..");
const workflowDir = path.join(root, ".github", "workflows");
const failures = [];
let checked = 0;

function checkFile(file) {
  const relative = path.relative(root, file).split(path.sep).join("/");
  const text = fs.readFileSync(file, "utf8");
  if (text.includes("\t")) {
    failures.push(`${relative}: contains a tab character`);
  }
  let doc;
  try {
    doc = YAML.parseDocument(text, { strict: true, uniqueKeys: true });
  } catch (error) {
    failures.push(`${relative}: parse error: ${error.message}`);
    return;
  }
  if (doc.errors.length > 0) {
    for (const error of doc.errors) failures.push(`${relative}: ${error.message}`);
    return;
  }
  const value = doc.toJS();
  if (!value || typeof value !== "object") {
    failures.push(`${relative}: not a mapping`);
    return;
  }
  if (relative.includes("/workflows/")) {
    if (!value.name) failures.push(`${relative}: missing top-level name`);
    if (!value.on) failures.push(`${relative}: missing 'on' trigger`);
    if (!value.jobs || Object.keys(value.jobs).length === 0) {
      failures.push(`${relative}: no jobs`);
    } else {
      for (const [id, job] of Object.entries(value.jobs)) {
        if (!job || typeof job !== "object") {
          failures.push(`${relative}: job '${id}' is not a mapping`);
          continue;
        }
        const runsOn = job["runs-on"];
        if (!runsOn) {
          if (!job.uses) failures.push(`${relative}: job '${id}' has no runs-on/uses`);
        }
        if (job.steps) {
          job.steps.forEach((step, index) => {
            if (typeof step !== "object") {
              failures.push(`${relative}: job '${id}' step ${index} is not a mapping`);
              return;
            }
            if (!step.uses && !step.run) {
              failures.push(`${relative}: job '${id}' step ${index} has neither uses nor run`);
            }
            if (step.uses && step.run) {
              failures.push(`${relative}: job '${id}' step ${index} has both uses and run`);
            }
          });
        }
      }
    }
  }
  checked += 1;
  console.log(`ok   ${relative}  (${doc.contents ? valueToSummary(value) : "empty"})`);
}

function valueToSummary(value) {
  const jobs = value.jobs ? Object.keys(value.jobs).join(", ") : "no jobs";
  return `jobs: ${jobs}`;
}

if (fs.existsSync(workflowDir)) {
  for (const file of fs.readdirSync(workflowDir)) {
    if (file.endsWith(".yml") || file.endsWith(".yaml")) {
      checkFile(path.join(workflowDir, file));
    }
  }
}
const dependabot = path.join(root, ".github", "dependabot.yml");
if (fs.existsSync(dependabot)) checkFile(dependabot);

if (failures.length > 0) {
  console.error(`\n${failures.length} problem(s):`);
  for (const failure of failures) console.error(`  - ${failure}`);
  process.exit(1);
}
console.log(`\n${checked} file(s) parsed cleanly.`);
