import { Marked } from 'marked';
import { markedTerminal } from 'marked-terminal';
import chalk from 'chalk';

// Configure marked for terminal output
const marked = new Marked();
marked.use(
  markedTerminal({
    // Code blocks
    code: chalk.bgGray.white,
    codespan: chalk.hex('#f472b6'),

    // Headings
    heading: chalk.bold.cyan,

    // Lists
    listitem: chalk.white,

    // Links
    link: chalk.blue.underline,

    // Emphasis
    strong: chalk.bold,
    em: chalk.italic,

    // Blockquotes
    blockquote: chalk.gray.italic,

    // Tables
    tableOptions: {
      chars: {
        top: '─',
        'top-mid': '┬',
        'top-left': '┌',
        'top-right': '┐',
        bottom: '─',
        'bottom-mid': '┴',
        'bottom-left': '└',
        'bottom-right': '┘',
        left: '│',
        'left-mid': '├',
        mid: '─',
        'mid-mid': '┼',
        right: '│',
        'right-mid': '┤',
        middle: '│',
      },
    },

    // Width
    width: 80,

    // Other
    reflowText: true,
    showSectionPrefix: false,
    tab: 2,
  }) as any
);

export function renderMarkdown(text: string): string {
  try {
    return marked.parse(text) as string;
  } catch {
    // Fall back to plain text if markdown parsing fails
    return text;
  }
}

// Simple code block formatter for streaming output
export function formatCodeBlock(code: string, language?: string): string {
  const header = language
    ? chalk.dim(`┌─ ${language} ${'─'.repeat(Math.max(0, 40 - language.length))}`)
    : chalk.dim('┌' + '─'.repeat(44));

  const footer = chalk.dim('└' + '─'.repeat(44));

  const lines = code.split('\n').map(line => chalk.dim('│ ') + chalk.white(line));

  return [header, ...lines, footer].join('\n');
}

// Format inline code
export function formatInlineCode(code: string): string {
  return chalk.bgGray.white(` ${code} `);
}

// Format a diff output
export function formatDiff(diff: string): string {
  return diff
    .split('\n')
    .map(line => {
      if (line.startsWith('+') && !line.startsWith('+++')) {
        return chalk.green(line);
      } else if (line.startsWith('-') && !line.startsWith('---')) {
        return chalk.red(line);
      } else if (line.startsWith('@@')) {
        return chalk.cyan(line);
      }
      return chalk.dim(line);
    })
    .join('\n');
}
