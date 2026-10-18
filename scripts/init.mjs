#!/usr/bin/env node
/** diligent init: create an audit workspace with frozen authority and rule locks. */
import { readFile, readdir, rm, mkdir } from 'node:fs/promises';
import { join, relative } from 'node:path';
import {
  ARTIFACTS, PACKAGE_ROOT, SCHEMA_VERSION, UsageError, appendEvent, assertAuditId, atomicWriteJson,
  newRunId, nowIso, pathExists, resolveRoot, sha256File, validateData
} from './lib/artifacts.mjs';
import { DILIGENT_VERSION, parseArgs, runMain } from './lib/cli.mjs';
import { buildInitialFacts } from './lib/scope-facts.mjs';
import { detectEngine } from './lib/engine.mjs';
import { isUnsupported, loadPack, loadProfile, loadRegistry } from './lib/data.mjs';

const MODES = ['source', 'page', 'journey', 'documents', 'combined'];
const HELP = `Usage: diligent init <audit-id> [--profile us-federal-public-web] [--mode source|page|journey|documents|combined]
                     [--root <workspace>] [--force] [--json]

Creates .diligent/<audit-id>/ with:
  SCOPE.json, SERVICE-BRIEF.json, PROJECT-CONTEXT.json (discovery pending),
  AUTHORITIES.lock.json and RULES.lock.json (frozen from the shipped data),
  EVENTS.jsonl, and runs/<run-id>/RUN.json.

Every scope fact starts as unknown with a question, an owner role, the
obligations it gates and its effect on the assessment. Nothing is assumed.

Options:
  --profile <id>   profile from profiles/ (default us-federal-public-web)
  --mode <modes>   one mode or a comma list: ${MODES.join(', ')} (default source)
  --root <dir>     workspace root (default: $DILIGENT_ROOT, nearest ancestor with .diligent/, or cwd)
  --data-root <d>  read authorities/rules/profiles from <d> instead of the installed package
  --force          replace an existing audit's artifacts (runs/ history and EVENTS.jsonl are kept)
  --json           machine-readable summary

Diligent ${DILIGENT_VERSION} ships zero automated website checks.`;

async function readPackageVersion() {
  return JSON.parse(await readFile(join(PACKAGE_ROOT, 'package.json'), 'utf8')).version;
}

async function checked(schema, data, label, errors) {
  const result = await validateData(schema, data, label);
  errors.push(...result.errors);
}

async function main() {
  const { positional, options } = parseArgs(process.argv.slice(2), {
    profile: 'string', mode: 'string', root: 'string', force: 'boolean', json: 'boolean', 'data-root': 'string'
  });
  if (options.help) { console.log(HELP); return 0; }
  if (positional.length !== 1) throw new UsageError('init requires exactly one <audit-id>. Run with --help.');
  const auditId = assertAuditId(positional[0]);
  const profileId = options.profile || 'us-federal-public-web';
  const modes = (options.mode || 'source').split(',').map(mode => mode.trim());
  for (const mode of modes) if (!MODES.includes(mode)) throw new UsageError(`unknown mode "${mode}". Use one of: ${MODES.join(', ')}.`);
  if (new Set(modes).size !== modes.length) throw new UsageError('--mode lists the same mode twice.');

  const root = resolveRoot({ root: options.root });
  const dir = join(root, '.diligent', auditId);
  const dataRoot = options['data-root'] || PACKAGE_ROOT;

  if ((await pathExists(dir)) && (await readdir(dir)).length > 0 && !options.force) {
    throw new UsageError(`${relative(root, dir) || dir} already exists. Use --force to replace its artifacts (runs/ history and EVENTS.jsonl are kept).`);
  }

  // Load and validate the shipped data before touching the workspace.
  const dataErrors = [];
  let profile; let registry; const packs = [];
  try { profile = await loadProfile(dataRoot, profileId); } catch (error) {
    if (error.code === 'ENOENT') throw new UsageError(`profile "${profileId}" does not exist under ${join(dataRoot, 'profiles')}.`);
    throw error;
  }
  registry = await loadRegistry(dataRoot);
  await checked('profile', profile.json, `profiles/${profileId}.json`, dataErrors);
  await checked('authority-registry', registry.json, 'authorities/registry.json', dataErrors);
  if (dataErrors.length === 0) {
    for (const packId of profile.json.selected_packs) {
      const pack = await loadPack(dataRoot, packId);
      await checked('rule-pack', pack.json, `rules/packs/${packId}.json`, dataErrors);
      packs.push({ id: packId, ...pack });
    }
  }
  if (dataErrors.length) {
    console.error('diligent init: shipped data failed validation; refusing to start an audit from invalid locks.');
    for (const line of dataErrors.slice(0, 20)) console.error(`  ${line}`);
    if (dataErrors.length > 20) console.error(`  ... and ${dataErrors.length - 20} more`);
    return 3;
  }

  const runId = newRunId();
  const startedAt = nowIso();
  const engine = detectEngine();
  const version = await readPackageVersion();
  const facts = buildInitialFacts();

  const toolVersions = {
    diligent: version, node: process.versions.node, schema_version: SCHEMA_VERSION,
    authority_registry: registry.json.registry_version
  };
  for (const pack of packs) toolVersions[`pack:${pack.id}`] = pack.json.pack_version;

  const schemaHashes = {};
  for (const name of (await readdir(join(PACKAGE_ROOT, 'schemas'))).filter(n => n.endsWith('.schema.json')).sort()) {
    schemaHashes[name] = await sha256File(join(PACKAGE_ROOT, 'schemas', name));
  }

  const run = {
    schema_version: SCHEMA_VERSION, run_id: runId, audit_id: auditId, profile_id: profileId, modes,
    requested_scope: { modes, profile_id: profileId, detail: 'none supplied; see SCOPE.json' },
    engine, tool_versions: toolVersions, started_at: startedAt, ended_at: null, snapshot_refs: [],
    pipeline_state: 'initialized', execution_status: 'initialized', assessment_complete: false, budgets: {},
    warnings: [`Diligent ${DILIGENT_VERSION} ships zero automated website checks. Every applicable obligation is untested until a human or documentary check supplies a result.`]
  };
  const scope = {
    schema_version: SCHEMA_VERSION, audit_id: auditId, run_id: runId, profile_id: profileId, modes, facts,
    permissions: {
      default: 'read_only', allowed_roots: [], allowed_origins: [], roles: [],
      prohibited_actions: ['live_submission', 'payment', 'record_change']
    },
    processing_boundary: { model_dispatch_allowed: false, network_allowed: false, notes: 'No remote model or network use is permitted until the owner records a boundary.' },
    source_deployment_relationship: null
  };
  const brief = {
    schema_version: SCHEMA_VERSION, audit_id: auditId, run_id: runId, brief_status: 'skeleton',
    service_id: null, service_name: null, owner: null, subject: null, audiences: [], jobs: [],
    critical_questions: [], journeys: [], failure_consequences: [], constraints: [], contexts: [],
    evidence: [], success_measures: [], non_goals: [], assumptions: []
  };
  const context = {
    schema_version: SCHEMA_VERSION, audit_id: auditId, run_id: runId, discovery_status: 'pending', identity: null,
    evidence_refs: [], unresolved_questions: [
      { question: 'What is this project, what is its purpose, and which agency or program governs it?', owner_role: 'program owner' }
    ], discovery_fingerprint: null
  };
  const lockedAt = startedAt;
  const authoritiesLock = {
    schema_version: SCHEMA_VERSION, run_id: runId, locked_at: lockedAt, source_mode: 'shipped_snapshot',
    source_path: 'authorities/registry.json', registry_version: registry.json.registry_version,
    registry_sha256: registry.sha256, authorities: registry.json.authorities
  };
  const matrix = [];
  for (const pack of packs) {
    for (const rule of pack.json.rules) {
      matrix.push({
        pack_id: pack.json.pack_id, rule_id: rule.rule_id, obligation_id: rule.obligation_id,
        lifecycle: rule.lifecycle, channel_support: rule.channel_support, unsupported: isUnsupported(rule.channel_support)
      });
    }
  }
  const rulesLock = {
    schema_version: SCHEMA_VERSION, run_id: runId, locked_at: lockedAt,
    profile: { profile_id: profileId, path: `profiles/${profileId}.json`, sha256: profile.sha256 },
    schema_hashes: schemaHashes,
    packs: packs.map(pack => ({
      pack_id: pack.json.pack_id, pack_version: pack.json.pack_version, path: `rules/packs/${pack.id}.json`,
      sha256: pack.sha256, rule_count: pack.json.rules.length, normative: pack.json.normative !== false
    })),
    supported_check_matrix: matrix
  };

  // Validate everything we are about to write. A failure here is a bug in init, not user error.
  const selfErrors = [];
  await checked('run', run, 'RUN.json', selfErrors);
  await checked('scope', scope, 'SCOPE.json', selfErrors);
  await checked('service-brief', brief, 'SERVICE-BRIEF.json', selfErrors);
  await checked('project-context', context, 'PROJECT-CONTEXT.json', selfErrors);
  await checked('authorities-lock', authoritiesLock, 'AUTHORITIES.lock.json', selfErrors);
  await checked('rules-lock', rulesLock, 'RULES.lock.json', selfErrors);
  if (selfErrors.length) {
    console.error('diligent init: generated artifacts failed validation; nothing was written.');
    for (const line of selfErrors.slice(0, 20)) console.error(`  ${line}`);
    return 3;
  }

  await mkdir(dir, { recursive: true });
  if (options.force) {
    for (const name of [...Object.keys(ARTIFACTS).filter(n => n !== 'EVENTS.jsonl'), 'REPORT.md', 'COMPONENT-GUIDANCE.md']) {
      await rm(join(dir, name), { force: true });
    }
  }

  const written = [];
  const write = async (relPath, data) => {
    const path = join(dir, relPath);
    await atomicWriteJson(path, data);
    written.push({ relPath, sha: await sha256File(path) });
  };
  await write(join('runs', runId, 'RUN.json'), run);
  await write('SCOPE.json', scope);
  await write('SERVICE-BRIEF.json', brief);
  await write('PROJECT-CONTEXT.json', context);
  await write('AUTHORITIES.lock.json', authoritiesLock);
  await write('RULES.lock.json', rulesLock);
  for (const { relPath, sha } of written) {
    await appendEvent(dir, {
      run_id: runId, stage: 'initialized',
      event: relPath.endsWith('RUN.json') ? 'audit_initialized' : 'artifact_written',
      artifact: relPath.split('\\').join('/'), sha256: sha
    });
  }

  const unresolved = facts.filter(fact => fact.state !== 'known').length;
  const summary = {
    audit_id: auditId, run_id: runId, directory: dir, profile_id: profileId, modes, engine: engine.name,
    unresolved_facts: unresolved, next_step: 'run discovery; see skills/discover',
    notice: `Diligent ${DILIGENT_VERSION} ships zero automated website checks.`
  };
  if (options.json) console.log(JSON.stringify(summary, null, 2));
  else {
    console.log(`Initialized audit "${auditId}" (${runId}) at ${dir}`);
    console.log(`Profile ${profileId}, mode ${modes.join('+')}, engine ${engine.name}.`);
    console.log(`${unresolved} scope facts are unresolved. Nothing has been checked.`);
    console.log('Next step: run discovery; see skills/discover');
    console.log(summary.notice);
  }
  return 0;
}

runMain(main);
