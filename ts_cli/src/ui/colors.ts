import chalk, { ChalkInstance } from 'chalk';

// Theme colors matching Claude/Codex aesthetic
export const theme = {
  // Primary colors
  primary: chalk.hex('#6366f1'),      // Indigo
  secondary: chalk.hex('#8b5cf6'),    // Purple
  accent: chalk.hex('#06b6d4'),       // Cyan

  // Status colors
  success: chalk.hex('#22c55e'),      // Green
  warning: chalk.hex('#f59e0b'),      // Amber
  error: chalk.hex('#ef4444'),        // Red
  info: chalk.hex('#3b82f6'),         // Blue

  // Text colors
  text: chalk.white,
  textDim: chalk.dim,
  textMuted: chalk.gray,

  // Special
  highlight: chalk.bold.cyan,
  code: chalk.hex('#f472b6'),         // Pink for code
  path: chalk.hex('#a78bfa'),         // Light purple for paths

  // Prompt styling
  prompt: chalk.bold.cyan('❯'),
  promptVoice: chalk.bold.magenta('🎤'),
  promptThinking: chalk.bold.yellow('◐'),
};

// Status indicators
export const icons = {
  success: chalk.green('✓'),
  error: chalk.red('✗'),
  warning: chalk.yellow('⚠'),
  info: chalk.blue('ℹ'),
  voice: chalk.magenta('🎤'),
  thinking: chalk.yellow('◐'),
  arrow: chalk.cyan('→'),
  bullet: chalk.dim('•'),
  diamond: chalk.cyan('◆'),
};

// Format helpers
export function formatPath(path: string): string {
  return theme.path(path);
}

export function formatCode(code: string): string {
  return theme.code(code);
}

export function formatCommand(cmd: string): string {
  return chalk.bold.white(cmd);
}

export function formatError(msg: string): string {
  return `${icons.error} ${theme.error(msg)}`;
}

export function formatSuccess(msg: string): string {
  return `${icons.success} ${theme.success(msg)}`;
}

export function formatWarning(msg: string): string {
  return `${icons.warning} ${theme.warning(msg)}`;
}

export function formatInfo(msg: string): string {
  return `${icons.info} ${theme.info(msg)}`;
}

// Styled output for different message types
export function userMessage(text: string): string {
  return `${theme.prompt} ${chalk.white(text)}`;
}

export function assistantMessage(text: string): string {
  return chalk.white(text);
}

export function systemMessage(text: string): string {
  return chalk.dim.italic(text);
}

export function voiceTranscript(text: string): string {
  return `${icons.voice} ${chalk.magenta(text)}`;
}
