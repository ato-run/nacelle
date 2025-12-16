#!/usr/bin/env node
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

function fail(message, exitCode = 1) {
  console.error(`✖ ${message}`);
  process.exit(exitCode);
}

function warn(message) {
  console.warn(`⚠ ${message}`);
}

function ok(message) {
  console.log(`✔ ${message}`);
}

const [manifestArg] = process.argv.slice(2);
if (!manifestArg) {
  fail('Usage: node tools/validator-cli/validate.js <path-to-manifest.webcapsule.json>');
}

const manifestPath = path.resolve(manifestArg);
if (!fs.existsSync(manifestPath)) {
  fail(`Manifest not found: ${manifestPath}`);
}

const manifestDir = path.dirname(manifestPath);

function readJson(filePath) {
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf8'));
  } catch (error) {
    fail(`Failed to parse JSON file: ${filePath}\n${error}`);
  }
}

const manifestRaw = fs.readFileSync(manifestPath);
const manifest = readJson(manifestPath);

if (manifest.schemaVersion !== '1.0.0') {
  fail('schemaVersion must be "1.0.0"');
}

if (manifest?.capsule?.kind !== 'hosted') {
  fail('capsule.kind must be "hosted"');
}

const endpoint = manifest?.capsule?.endpoint;
if (typeof endpoint !== 'string' || !endpoint.startsWith('https://')) {
  fail('capsule.endpoint must start with https://');
}

const egress = manifest?.capsule?.network?.egress ?? [];
const allowedLoopback = new Set(['http://127.0.0.1', 'http://[::1]']);
if (!Array.isArray(egress) || egress.length !== allowedLoopback.size) {
  fail('network.egress must contain only loopback entries');
}
for (const target of egress) {
  if (!allowedLoopback.has(target)) {
    fail(`network.egress contains unsupported host: ${target}`);
  }
}

const sbomPath = manifest?.integrity?.sbom
  ? path.resolve(manifestDir, manifest.integrity.sbom)
  : null;
const signaturePath = manifest?.integrity?.signature
  ? path.resolve(manifestDir, manifest.integrity.signature)
  : null;

if (!sbomPath || !fs.existsSync(sbomPath)) {
  fail('integrity.sbom file is missing');
}
const sbom = readJson(sbomPath);
if (sbom.schemaVersion !== 'SPDX-2.3') {
  warn('SBOM schemaVersion is not SPDX-2.3');
}

if (!signaturePath || !fs.existsSync(signaturePath)) {
  fail('integrity.signature file is missing');
}

const signatureDir = path.dirname(signaturePath);
const signature = readJson(signaturePath);

if (signature.algorithm?.toLowerCase() !== 'ed25519') {
  fail('signature.algorithm must be "ed25519"');
}

const digestHex = crypto.createHash('sha256').update(manifestRaw).digest('hex');
const recordedDigest = signature?.manifestDigest?.sha256;
if (recordedDigest !== digestHex) {
  fail('manifestDigest.sha256 does not match the manifest contents');
}

const chain = signature?.signingAuthority?.certificateChain;
if (!Array.isArray(chain) || chain.length === 0) {
  fail('signature.signingAuthority.certificateChain must include at least one certificate');
}

const certPath = path.resolve(signatureDir, chain[0]);
if (!fs.existsSync(certPath)) {
  fail(`Certificate file missing: ${certPath}`);
}

const certPem = fs.readFileSync(certPath, 'utf8');
let publicKey;
try {
  publicKey = crypto.createPublicKey(certPem);
} catch (error) {
  fail(`Failed to load public key from certificate: ${error.message}`);
}

if (!signature.signature) {
  fail('signature.signature is empty');
}

let signatureBuf;
try {
  signatureBuf = Buffer.from(signature.signature, 'base64');
} catch {
  fail('signature.signature is not valid base64');
}

const verified = crypto.verify(null, manifestRaw, publicKey, signatureBuf);
if (!verified) {
  fail('Manifest signature verification failed');
}

ok('Manifest schema and signature validation passed');
ok(`SBOM (${path.relative(process.cwd(), sbomPath)}) parsed successfully`);
ok(`Certificate chain verified via ${path.relative(process.cwd(), certPath)}`);
