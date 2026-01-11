import ora, { Ora } from 'ora';
import chalk from 'chalk';

// Custom spinner frames
const voiceFrames = ['🎤', '🎙️', '🎤', '🎙️'];
const thinkingFrames = ['◐', '◓', '◑', '◒'];
const dotsFrames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

export type SpinnerType = 'voice' | 'thinking' | 'loading' | 'dots' | 'auth';

class SpinnerManager {
  private spinner: Ora | null = null;

  start(type: SpinnerType, text: string): void {
    this.stop();

    const config = this.getConfig(type);
    this.spinner = ora({
      text: chalk.dim(text),
      spinner: config.spinner,
      color: config.color as any,
      discardStdin: false,  // DON'T consume stdin - we're using raw mode
    }).start();
  }

  update(text: string): void {
    if (this.spinner) {
      this.spinner.text = chalk.dim(text);
    }
  }

  succeed(text: string): void {
    if (this.spinner) {
      this.spinner.succeed(chalk.green(text));
      this.spinner = null;
    }
  }

  fail(text: string): void {
    if (this.spinner) {
      this.spinner.fail(chalk.red(text));
      this.spinner = null;
    }
  }

  warn(text: string): void {
    if (this.spinner) {
      this.spinner.warn(chalk.yellow(text));
      this.spinner = null;
    }
  }

  stop(): void {
    if (this.spinner) {
      this.spinner.stop();
      this.spinner = null;
    }
  }

  private getConfig(type: SpinnerType): { spinner: { frames: string[], interval: number }, color: string } {
    switch (type) {
      case 'voice':
        return {
          spinner: { frames: voiceFrames, interval: 300 },
          color: 'magenta',
        };
      case 'thinking':
        return {
          spinner: { frames: thinkingFrames, interval: 150 },
          color: 'yellow',
        };
      case 'dots':
        return {
          spinner: { frames: dotsFrames, interval: 80 },
          color: 'cyan',
        };
      case 'auth':
        return {
          spinner: { frames: dotsFrames, interval: 100 },
          color: 'green',
        };
      case 'loading':
      default:
        return {
          spinner: { frames: dotsFrames, interval: 80 },
          color: 'blue',
        };
    }
  }
}

export const spinner = new SpinnerManager();

// Simple inline spinner for streaming output
export function createInlineSpinner(): { frame: () => string; stop: () => void } {
  let index = 0;
  let stopped = false;

  return {
    frame: () => {
      if (stopped) return '';
      const frame = thinkingFrames[index % thinkingFrames.length];
      index++;
      return chalk.yellow(frame);
    },
    stop: () => {
      stopped = true;
    },
  };
}
