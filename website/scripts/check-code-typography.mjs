import { readdir, readFile } from 'node:fs/promises';
import { extname, relative, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const forbiddenCharacters = new Map([
  ['\u2013', 'en dash'],
  ['\u201c', 'left curly double quotation mark'],
  ['\u201d', 'right curly double quotation mark'],
]);

export function findCodeTypographyViolations(source) {
  const lines = source.split(/\r\n|\n|\r/);
  const violations = [];
  let fence = null;
  let proseLines = [];

  const reportLine = (text, line, columnOffset = 0) => {
    for (let column = 0; column < text.length; column += 1) {
      const description = forbiddenCharacters.get(text[column]);
      if (description) {
        violations.push({ line, column: column + columnOffset + 1, character: text[column], description });
      }
    }
  };

  const flushProse = () => {
    if (proseLines.length > 0) {
      scanInlineCode(proseLines, violations);
      proseLines = [];
    }
  };

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];

    if (fence) {
      if (isFenceClose(line, fence)) {
        fence = null;
      } else {
        reportLine(line, index + 1);
      }
      continue;
    }

    const openingFence = parseFenceOpening(line);
    if (openingFence) {
      flushProse();
      fence = openingFence;
      continue;
    }

    const indentation = line.match(/^( {4}|\t)/)?.[0];
    if (indentation) {
      flushProse();
      reportLine(line.slice(indentation.length), index + 1, indentation.length);
      continue;
    }

    if (line.trim() === '') {
      flushProse();
      continue;
    }

    proseLines.push({ text: line, number: index + 1 });
  }

  flushProse();
  return violations;
}

function parseFenceOpening(line) {
  const match = /^( {0,3})(`{3,}|~{3,})(.*)$/.exec(line);
  if (!match || (match[2][0] === '`' && match[3].includes('`'))) {
    return null;
  }

  return { character: match[2][0], length: match[2].length };
}

function isFenceClose(line, fence) {
  const match = /^ {0,3}(`+|~+)[\t ]*$/.exec(line);
  return match && match[1][0] === fence.character && match[1].length >= fence.length;
}

function scanInlineCode(lines, violations) {
  const source = lines.map(({ text }) => text).join('\n');
  const lineStarts = [];
  let lineStart = 0;
  for (const { text } of lines) {
    lineStarts.push(lineStart);
    lineStart += text.length + 1;
  }

  const reportRange = (start, end) => {
    for (let offset = start; offset < end; offset += 1) {
      const description = forbiddenCharacters.get(source[offset]);
      if (!description) {
        continue;
      }

      const lineIndex = findLineIndex(lineStarts, offset);
      violations.push({
        line: lines[lineIndex].number,
        column: offset - lineStarts[lineIndex] + 1,
        character: source[offset],
        description,
      });
    }
  };

  let index = 0;
  while (index < source.length) {
    if (source[index] === '`' && !isEscaped(source, index)) {
      const length = countRun(source, index, '`');
      const closing = findClosingBackticks(source, index + length, length);
      if (closing !== -1) {
        reportRange(index + length, closing);
        index = closing + length;
      } else {
        index += length;
      }
      continue;
    }

    if (source[index] === '<') {
      const opening = /^<(code|pre)(?=\s|>)[^>]*>/i.exec(source.slice(index));
      if (opening) {
        const tag = opening[1];
        const contentStart = index + opening[0].length;
        const closingPattern = new RegExp(`</${tag}\\s*>`, 'ig');
        closingPattern.lastIndex = contentStart;
        const closing = closingPattern.exec(source);
        if (closing) {
          reportRange(contentStart, closing.index);
          index = closing.index + closing[0].length;
          continue;
        }
      }
    }

    index += 1;
  }
}

function countRun(source, start, character) {
  let end = start;
  while (source[end] === character) {
    end += 1;
  }
  return end - start;
}

function findClosingBackticks(source, start, length) {
  let index = start;
  while ((index = source.indexOf('`', index)) !== -1) {
    const runLength = countRun(source, index, '`');
    if (runLength === length) {
      return index;
    }
    index += runLength;
  }
  return -1;
}

function isEscaped(source, index) {
  let backslashes = 0;
  for (let cursor = index - 1; cursor >= 0 && source[cursor] === '\\'; cursor -= 1) {
    backslashes += 1;
  }
  return backslashes % 2 === 1;
}

function findLineIndex(lineStarts, offset) {
  let low = 0;
  let high = lineStarts.length;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    if (lineStarts[middle] <= offset) {
      low = middle + 1;
    } else {
      high = middle;
    }
  }
  return low - 1;
}

async function findMarkdownFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...await findMarkdownFiles(path));
    } else if (entry.isFile() && ['.md', '.mdx'].includes(extname(entry.name).toLowerCase())) {
      files.push(path);
    }
  }
  return files.sort();
}

async function checkDocumentation(root) {
  const files = await findMarkdownFiles(root);
  if (files.length === 0) {
    throw new Error(`No Markdown/MDX files found under ${root}`);
  }

  let violationCount = 0;

  for (const file of files) {
    const source = await readFile(file, 'utf8');
    for (const violation of findCodeTypographyViolations(source)) {
      console.error(
        `${relative(process.cwd(), file)}:${violation.line}:${violation.column}: ${violation.description} is not allowed in code`,
      );
      violationCount += 1;
    }
  }

  if (violationCount > 0) {
    console.error(`Found ${violationCount} prohibited character(s) in code.`);
    process.exitCode = 1;
  } else {
    console.log(`Code typography check passed for ${files.length} Markdown/MDX files.`);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  await checkDocumentation(resolve(process.argv[2] ?? 'src/content/docs'));
}
