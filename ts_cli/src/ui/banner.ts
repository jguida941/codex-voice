import gradient from 'gradient-string';
import chalk from 'chalk';

// Custom gradient that looks like Claude/Codex branding
const brandGradient = gradient(['#6366f1', '#8b5cf6', '#a855f7']);
const accentGradient = gradient(['#06b6d4', '#3b82f6', '#6366f1']);

export const BANNER = `
   ██████╗ ██████╗ ██████╗ ███████╗██╗  ██╗
  ██╔════╝██╔═══██╗██╔══██╗██╔════╝╚██╗██╔╝
  ██║     ██║   ██║██║  ██║█████╗   ╚███╔╝
  ██║     ██║   ██║██║  ██║██╔══╝   ██╔██╗
  ╚██████╗╚██████╔╝██████╔╝███████╗██╔╝ ██╗
   ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝╚═╝  ╚═╝
          ██╗   ██╗ ██████╗ ██╗ ██████╗███████╗
          ██║   ██║██╔═══██╗██║██╔════╝██╔════╝
          ██║   ██║██║   ██║██║██║     █████╗
          ╚██╗ ██╔╝██║   ██║██║██║     ██╔══╝
           ╚████╔╝ ╚██████╔╝██║╚██████╗███████╗
            ╚═══╝   ╚═════╝ ╚═╝ ╚═════╝╚══════╝
`;

export const SMALL_BANNER = `
  ┌─────────────────────────────────────┐
  │  ◆ CODEX VOICE                      │
  │    AI-powered coding with voice     │
  └─────────────────────────────────────┘
`;

export function printBanner(small = false): void {
  if (small) {
    console.log(accentGradient(SMALL_BANNER));
  } else {
    console.log(brandGradient(BANNER));
  }
}

export function printWelcome(): void {
  const lines = [
    '',
    chalk.bold.cyan('  Welcome to Codex Voice'),
    chalk.dim('  ─────────────────────────────────────'),
    '',
    `  ${chalk.yellow('◆')} Type a prompt and press ${chalk.bold('Enter')} to send`,
    `  ${chalk.yellow('◆')} Press ${chalk.bold('Ctrl+R')} to use voice input`,
    `  ${chalk.yellow('◆')} Press ${chalk.bold('Ctrl+V')} to toggle auto-voice mode`,
    `  ${chalk.yellow('◆')} Type ${chalk.bold('/help')} for available commands`,
    '',
  ];
  lines.forEach(line => console.log(line));
}

export function printDivider(char = '─', width = 50): void {
  console.log(chalk.dim(char.repeat(width)));
}

export function printSection(title: string): void {
  console.log('');
  console.log(chalk.bold.cyan(`  ◆ ${title}`));
  printDivider();
}
