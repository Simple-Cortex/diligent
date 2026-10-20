/**
 * Vite plugin that stamps native elements with the place they were written:
 * data-diligent-src="<path relative to root>:<line>".
 *
 * Works on JSX/TSX, Vue (outside script and style blocks), Svelte and Astro
 * sources. It lexes tags tolerantly, with no dependencies and no parser, the
 * same way the Rust source pass reads template files. Components (uppercase or
 * dotted names), fragments, built-in framework elements and tags that already
 * carry the attribute are left alone. It produces no source map: the attribute
 * goes right after the tag name, so only that line shifts, to the right.
 *
 * Private to this repository; not published, not in the npm package.
 */
import { relative, sep } from 'node:path';

export const ATTRIBUTE = 'data-diligent-src';

const EXTENSIONS = new Map([
  ['.jsx', 'jsx'],
  ['.tsx', 'jsx'],
  ['.vue', 'vue'],
  ['.svelte', 'svelte'],
  ['.astro', 'astro']
]);

const VOID = new Set([
  'area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input', 'link', 'meta', 'param', 'source',
  'track', 'wbr'
]);

// Never stamped: raw text, and elements the frameworks treat specially.
const SKIP_TAGS = new Set([
  'script', 'style', 'template', 'slot', 'component', 'transition', 'transition-group',
  'keep-alive', 'teleport', 'suspense'
]);

const MAX_DEPTH = 64;

function isWs(c) {
  return c === ' ' || c === '\t' || c === '\n' || c === '\r' || c === '\f';
}

function isAlpha(c) {
  return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z');
}

function isWordChar(c) {
  return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c === '_' || c === '$';
}

/** Start offset of each line. A line ends at \n, \r\n or a lone \r. */
function lineStarts(code) {
  const starts = [0];
  for (let i = 0; i < code.length; i++) {
    const c = code[i];
    if (c === '\n') starts.push(i + 1);
    else if (c === '\r' && code[i + 1] !== '\n') starts.push(i + 1);
  }
  return starts;
}

function lineOf(starts, offset) {
  let lo = 0;
  let hi = starts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (starts[mid] <= offset) lo = mid;
    else hi = mid - 1;
  }
  return lo + 1;
}

/** Index just past the JS string or template literal starting at i. */
function stringEnd(code, i, depth = 0) {
  const q = code[i];
  let j = i + 1;
  while (j < code.length) {
    const c = code[j];
    if (c === '\\') j += 2;
    else if (c === q) return j + 1;
    else if (c === '\n' && q !== '`') return j;
    else if (q === '`' && c === '$' && code[j + 1] === '{') j = braceEnd(code, j + 1, depth + 1) + 1;
    else j++;
  }
  return code.length;
}

/** Index of the } that closes the { at i, or the text length. */
function braceEnd(code, i, depth = 0) {
  if (depth > MAX_DEPTH) return code.length;
  let level = 0;
  let j = i;
  while (j < code.length) {
    const c = code[j];
    if (c === '{') level++;
    else if (c === '}') {
      level--;
      if (level <= 0) return j;
    } else if (c === '"' || c === "'" || c === '`') {
      j = stringEnd(code, j, depth);
      continue;
    }
    j++;
  }
  return code.length;
}

const KEYWORDS_BEFORE_JSX = new Set([
  'return', 'default', 'case', 'yield', 'await', 'in', 'of', 'else', 'do', 'typeof', 'void', 'throw'
]);

/** True when the text before i lets a JSX element start at i. */
function jsxAllowedBefore(code, i) {
  let j = i;
  while (j > 0 && isWs(code[j - 1])) j--;
  if (j === 0) return true;
  const p = code[j - 1];
  if (isWordChar(p)) {
    let k = j;
    while (k > 0 && isWordChar(code[k - 1])) k--;
    return KEYWORDS_BEFORE_JSX.has(code.slice(k, j));
  }
  return p !== ')' && p !== ']' && p !== '.';
}

function indexOfCI(code, needle, from) {
  const lower = code.toLowerCase();
  return lower.indexOf(needle, from);
}

/**
 * Reads a start tag at i (code[i] === '<', next is a letter). Returns
 * { name, nameEnd, end, selfClosing } where end is just past the '>'.
 */
function readTag(code, i, flavor) {
  let nameEnd = i + 1;
  while (nameEnd < code.length && !isWs(code[nameEnd]) && code[nameEnd] !== '/' && code[nameEnd] !== '>') nameEnd++;
  const name = code.slice(i + 1, nameEnd);
  let j = nameEnd;
  let selfClosing = false;
  while (j < code.length) {
    const c = code[j];
    if (c === '>') {
      j++;
      return { name, nameEnd, end: j, selfClosing };
    }
    if (c === '/' && code[j + 1] === '>') {
      return { name, nameEnd, end: j + 2, selfClosing: true };
    }
    if (c === '"' || c === "'") {
      const close = code.indexOf(c, j + 1);
      j = close === -1 ? code.length : close + 1;
    } else if (c === '{' && flavor !== 'vue') {
      j = Math.min(braceEnd(code, j) + 1, code.length);
    } else {
      j++;
    }
  }
  return { name, nameEnd, end: code.length, selfClosing, unterminated: true };
}

function hasAttribute(tagText) {
  return new RegExp(`[\\s"'}]${ATTRIBUTE}(?=[\\s=/>]|$)`).test(tagText);
}

function isNative(name) {
  return /^[a-z][a-z0-9-]*$/.test(name) && !SKIP_TAGS.has(name);
}

/** End of the leading --- frontmatter block in an Astro file, or 0. */
function astroFrontmatterEnd(code) {
  const m = /^(﻿?\s*)---[ \t]*\r?\n/.exec(code);
  if (!m) return 0;
  const close = /\r?\n---[ \t]*(\r?\n|$)/.exec(code.slice(m[0].length - 1));
  if (!close) return 0;
  return m[0].length - 1 + close.index + close[0].length;
}

/**
 * Finds the offset where each stampable tag's name ends.
 * Returns [{ offset: nameEnd, tagStart }].
 */
export function findStampPoints(code, flavor) {
  const points = [];
  const stack = [{ kind: flavor === 'jsx' ? 'js' : 'root', depth: 0, braces: 0 }];
  let i = flavor === 'astro' ? astroFrontmatterEnd(code) : 0;
  const top = () => stack[stack.length - 1];

  const handleTag = (at) => {
    const next = code[at + 1];
    if (next === '>') {
      // Fragment: opens an element, never stamped.
      if (top().kind === 'js') stack.push({ kind: 'markup', depth: 1, braces: 0 });
      else top().depth++;
      return at + 2;
    }
    const tag = readTag(code, at, flavor);
    const lower = tag.name.toLowerCase();
    const rawText = (flavor !== 'jsx') && (lower === 'script' || lower === 'style');
    if (rawText) {
      const close = indexOfCI(code, `</${lower}`, tag.end);
      if (close === -1) return code.length;
      const gt = code.indexOf('>', close);
      return gt === -1 ? code.length : gt + 1;
    }
    const tagText = code.slice(at, tag.end);
    if (isNative(tag.name) && !tag.unterminated && !hasAttribute(tagText)) {
      points.push({ offset: tag.nameEnd, tagStart: at });
    }
    const opens = !tag.selfClosing && !(flavor !== 'jsx' && VOID.has(lower));
    if (opens) {
      if (top().kind === 'js') stack.push({ kind: 'markup', depth: 1, braces: 0 });
      else top().depth++;
    } else if (top().kind === 'js') {
      // A lone self-closing element inside an expression: nothing to track.
    }
    return tag.end;
  };

  const handleEndTag = (at) => {
    const gt = code.indexOf('>', at);
    const t = top();
    if (t.kind === 'markup') {
      t.depth--;
      if (t.depth <= 0) stack.pop();
    } else if (t.kind === 'root') {
      t.depth = Math.max(0, t.depth - 1);
    }
    return gt === -1 ? code.length : gt + 1;
  };

  while (i < code.length) {
    const t = top();
    const c = code[i];
    const inMarkup = t.kind !== 'js';
    if (inMarkup) {
      if (c === '<') {
        if (code.startsWith('<!--', i)) {
          const end = code.indexOf('-->', i + 4);
          i = end === -1 ? code.length : end + 3;
        } else if (code[i + 1] === '/') {
          i = handleEndTag(i);
        } else if (isAlpha(code[i + 1] ?? '') || code[i + 1] === '>') {
          i = handleTag(i);
        } else {
          i++;
        }
      } else if (c === '{' && flavor === 'vue' && code[i + 1] === '{') {
        const end = code.indexOf('}}', i + 2);
        i = end === -1 ? code.length : end + 2;
      } else if (c === '{' && flavor !== 'vue') {
        const n = code[i + 1];
        if (flavor === 'svelte' && (n === '#' || n === ':' || n === '/' || n === '@')) {
          i = Math.min(braceEnd(code, i) + 1, code.length);
        } else if (stack.length < MAX_DEPTH) {
          stack.push({ kind: 'js', depth: 0, braces: 0 });
          i++;
        } else {
          i++;
        }
      } else {
        i++;
      }
      continue;
    }
    // JavaScript context: top level of a JSX file, or inside { }.
    if (c === '"' || c === "'" || c === '`') {
      i = stringEnd(code, i);
    } else if (c === '/' && code[i + 1] === '/') {
      const nl = code.indexOf('\n', i);
      i = nl === -1 ? code.length : nl;
    } else if (c === '/' && code[i + 1] === '*') {
      const end = code.indexOf('*/', i + 2);
      i = end === -1 ? code.length : end + 2;
    } else if (c === '{') {
      t.braces++;
      i++;
    } else if (c === '}') {
      if (t.braces === 0 && stack.length > 1) stack.pop();
      else t.braces = Math.max(0, t.braces - 1);
      i++;
    } else if (c === '<' && (isAlpha(code[i + 1] ?? '') || code[i + 1] === '>') && jsxAllowedBefore(code, i)) {
      i = handleTag(i);
    } else {
      i++;
    }
  }
  return points;
}

/**
 * Adds data-diligent-src to every native element in code. `file` is the path
 * written into the attribute. Returns the new code, or null when nothing changed.
 */
export function stampSource(code, flavor, file) {
  const points = findStampPoints(code, flavor);
  if (points.length === 0) return null;
  const starts = lineStarts(code);
  const safe = file.split(sep).join('/').replace(/"/g, '&quot;');
  let out = '';
  let last = 0;
  for (const { offset, tagStart } of points) {
    out += code.slice(last, offset);
    out += ` ${ATTRIBUTE}="${safe}:${lineOf(starts, tagStart)}"`;
    last = offset;
  }
  out += code.slice(last);
  return out;
}

/**
 * @param {{ enabled?: boolean, root?: string }} [options]
 *   enabled: stamp files. Default: true when Vite's mode is "development".
 *   root: paths in the attribute are relative to this. Default: Vite's root.
 */
export function diligentSrc(options = {}) {
  let enabled = options.enabled;
  let root = options.root;
  return {
    name: 'diligent-src',
    enforce: 'pre',
    configResolved(config) {
      if (enabled === undefined) enabled = config.mode === 'development';
      if (root === undefined) root = config.root;
    },
    transform(code, id) {
      if (enabled !== true) return null;
      const queryAt = id.indexOf('?');
      const file = queryAt === -1 ? id : id.slice(0, queryAt);
      const query = queryAt === -1 ? '' : id.slice(queryAt + 1);
      // Compiled sub-requests of a single-file component are not source.
      if (query.split('&').some(part => part === 'vue' || part === 'raw' || part === 'url' || part.startsWith('type='))) return null;
      const dot = file.lastIndexOf('.');
      const flavor = dot === -1 ? undefined : EXTENSIONS.get(file.slice(dot).toLowerCase());
      if (!flavor || file.includes('node_modules')) return null;
      const shown = root ? relative(root, file) : file;
      const out = stampSource(code, flavor, shown);
      return out === null ? null : { code: out, map: null };
    }
  };
}

export default diligentSrc;
