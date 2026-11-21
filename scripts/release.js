// bumps the release number, commits, pushes and starts the CI build, which tags vN once it publishes the release.
// --undo takes the latest release back: cancels its build, deletes its release and tag, and reverts the version commit.
// tauri needs semver so package.json holds 0.0.N while the tag is just vN
import { execSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { createInterface } from 'node:readline/promises';

// the account that owns the repo, so releases start from it whichever gh account is active
const GITHUB_ACCOUNT = 'ikaikahub';

/** @param {string} command */
const git = (command) => execSync(`git ${command}`, { encoding: 'utf8' }).trim();

const token = execSync(`gh auth token --user ${GITHUB_ACCOUNT}`, { encoding: 'utf8' }).trim();
// show prints gh's own output, like the link to a started build
/** @param {string} command */
const gh = (command, show = false) =>
  execSync(`gh ${command}`, {
    encoding: 'utf8',
    stdio: ['ignore', show ? 'inherit' : 'pipe', show ? 'inherit' : 'pipe'],
    env: { ...process.env, GH_TOKEN: token },
  })?.trim();

// pnpm re-installs before the next script whenever package.json changes, and that install hangs when run that way.
// installing here instead records the new version, so later scripts start right away
const recordVersion = () => execSync('pnpm install --frozen-lockfile', { stdio: ['ignore', 'inherit', 'inherit'] });

const undoing = process.argv.includes('--undo');
// a release builds what's pushed, so everything has to be committed. undoing only reverts package.json,
// which works around other uncommitted work as long as nothing is staged to slip into the revert commit
const blocking = undoing
  ? git('status --porcelain -- package.json') || git('diff --cached --name-only')
  : git('status --porcelain --untracked-files=no');
if (blocking) {
  console.error(undoing ? 'package.json is changed or something is staged, commit or stash it first' : 'commit or stash your changes first');
  process.exit(1);
}

const packagePath = new URL('../package.json', import.meta.url);
const packageText = readFileSync(packagePath, 'utf8');
const currentRelease = Number(JSON.parse(packageText).version.split('.')[2]);

if (undoing) {
  await undo(`v${currentRelease}`);
} else {
  release(currentRelease + 1);
}

// the commit messages since the newest published release, without the vN version commits or their reverts
function releaseNotes() {
  git('fetch --tags --quiet');
  let previous = '';
  try {
    previous = git('describe --tags --abbrev=0 --match v* HEAD');
  } catch {
    // no release yet, every commit counts
  }
  const messages = git(`log --no-merges --format=%s ${previous ? `${previous}..HEAD` : 'HEAD'}`)
    .split('\n')
    .map((message) => message.trim())
    // version bumps and their undos say nothing about what changed
    .filter((message) => message && !/^(Revert "v\d+"|v\d+)$/.test(message));
  return messages.length ? messages.map((message) => `- ${message}`).join('\n') : '- Small fixes';
}

/** @param {number} number */
function release(number) {
  const notes = releaseNotes();
  console.log(`notes for v${number}:\n${notes}`);
  writeFileSync(packagePath, packageText.replace(/"version": "[^"]*"/, `"version": "0.0.${number}"`));
  recordVersion();

  const tag = `v${number}`;
  const branch = git('branch --show-current');
  git('add package.json');
  git(`commit -m ${tag}`);
  // -u origin HEAD also works for a branch that hasn't been pushed yet
  git('push -u origin HEAD');

  // a run on the branch shares its build cache with every other release, a run on a fresh tag starts from nothing
  // --json takes the inputs on stdin, so quotes and symbols in commit messages can't break the command
  execSync(`gh workflow run build.yml --ref ${branch} --json`, {
    input: JSON.stringify({ release_tag: tag, notes }),
    stdio: ['pipe', 'inherit', 'inherit'],
    env: { ...process.env, GH_TOKEN: token },
  });
  console.log(`started the ${tag} build on ${branch}`);
}

/** @param {string} tag */
async function undo(tag) {
  const commit = git(`log -n 1 --format=%H --grep=^${tag}$ -- package.json`);
  if (!commit) {
    console.error(`couldn't find the ${tag} version commit`);
    process.exit(1);
  }
  const prompt = createInterface({ input: process.stdin, output: process.stdout });
  const answer = await prompt.question(
    `Undo ${tag}? This cancels its build, deletes its GitHub release and tag, and reverts commit ${commit.slice(0, 7)}. [y/N] `,
  );
  prompt.close();
  if (answer.trim().toLowerCase() !== 'y') {
    console.log('nothing changed');
    return;
  }

  /** @type {{ databaseId: number, displayTitle: string, status: string }[]} */
  const runs = JSON.parse(gh('run list --workflow build.yml --limit 20 --json databaseId,displayTitle,status'));
  for (const run of runs.filter((run) => run.displayTitle === tag && run.status !== 'completed')) {
    gh(`run cancel ${run.databaseId}`);
    console.log(`cancelled build ${run.databaseId}`);
  }

  // a build that finished made a release with the tag, a build that failed late may have left only the tag
  try {
    gh(`release delete ${tag} --cleanup-tag --yes`);
    console.log(`deleted the ${tag} release and tag`);
  } catch {
    try {
      gh(`api -X DELETE repos/{owner}/{repo}/git/refs/tags/${tag}`);
      console.log(`deleted the ${tag} tag`);
    } catch {
      console.log(`there was no ${tag} release or tag`);
    }
  }

  git(`revert --no-edit ${commit}`);
  recordVersion();
  git('push');
  console.log(`reverted ${tag}, package.json is back to 0.0.${currentRelease - 1}`);
}
