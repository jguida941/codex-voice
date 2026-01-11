#!/usr/bin/env node

import * as readline from 'readline';
import chalk from 'chalk';
import { printBanner, printWelcome } from './ui/banner.js';
import { theme, icons, formatError, formatSuccess, formatInfo, voiceTranscript } from './ui/colors.js';
import { spinner } from './ui/spinner.js';
import { RustBridge, CapabilitiesEvent, JobEndEvent, JobStartEvent } from './bridge/rust-ipc.js';

// State
let isProcessing = false;
let autoVoiceMode = false;
let rl: readline.Interface;
let bridge: RustBridge | null = null;
let backendReady = false;
let capabilities: CapabilitiesEvent | null = null;
let activeProvider = 'codex';
let rawLineBuffer = '';
let rawModeEnabled = false;
let rawInputHandler: ((key: Buffer) => void) | null = null;

// Show the input prompt
function showPrompt(): void {
  const providerIndicator = activeProvider === 'claude' ? chalk.magenta('[claude]') : chalk.cyan('[codex]');
  const prefix = autoVoiceMode ? icons.voice + ' ' : '';
  const promptStr = `${prefix}${providerIndicator} ${theme.prompt} `;

  if (rl) {
    rl.setPrompt(promptStr);
    rl.prompt();
  } else {
    // Raw mode - just print the prompt
    process.stdout.write(promptStr);
  }
}

// Handle voice capture (Ctrl+R or /voice)
function handleVoiceCapture(): void {
  if (isProcessing) {
    console.log(formatInfo('Please wait for the current operation to complete'));
    showPrompt();
    return;
  }

  if (!backendReady) {
    console.log(formatError('Voice backend not ready'));
    showPrompt();
    return;
  }

  if (!capabilities?.mic_available) {
    console.log(formatError('No microphone available'));
    showPrompt();
    return;
  }

  if (!capabilities?.whisper_model_loaded) {
    console.log(formatError('Whisper model not loaded - voice input unavailable'));
    console.log(chalk.dim('  Run: ./scripts/setup.sh models --base'));
    showPrompt();
    return;
  }

  isProcessing = true;
  spinner.start('voice', 'Listening... (speak now, will auto-stop after silence)');
  bridge?.startVoice();
}

// Toggle auto-voice mode
function toggleAutoVoice(): void {
  autoVoiceMode = !autoVoiceMode;
  if (autoVoiceMode) {
    console.log(formatInfo('Auto-voice mode enabled - voice input will auto-start after responses'));
  } else {
    console.log(formatInfo('Auto-voice mode disabled - press Enter to send, Ctrl+R for voice'));
  }
  showPrompt();
}

// Handle user input
async function handleInput(input: string): Promise<void> {
  const trimmed = input.trim();

  if (!trimmed) {
    showPrompt();
    return;
  }

  // Handle slash commands
  if (trimmed.startsWith('/')) {
    handleCommand(trimmed);
    return;
  }

  // Send to backend if available
  if (backendReady && bridge) {
    await sendPrompt(trimmed);
  } else {
    console.log(formatInfo(`You said: "${trimmed}"`));
    console.log(chalk.dim('(Backend not connected - running in demo mode)'));
    showPrompt();
  }
}

// Handle slash commands
function handleCommand(input: string): void {
  // Stop any active spinner before showing command output
  spinner.stop();

  const parts = input.slice(1).split(/\s+/);
  const cmd = parts[0]?.toLowerCase();
  const args = parts.slice(1).join(' ');

  switch (cmd) {
    case 'help':
    case 'h':
      printHelp();
      showPrompt();
      break;

    case 'clear':
    case 'cls':
      console.clear();
      printBanner(true);
      showPrompt();
      break;

    case 'voice':
    case 'v':
      handleVoiceCapture();
      break;

    case 'auto':
      toggleAutoVoice();
      break;

    case 'status':
      printStatus();
      showPrompt();
      break;

    case 'provider':
      handleProviderCommand(args);
      break;

    case 'auth':
      if (!backendReady || !bridge) {
        console.log(formatError('Backend not ready'));
        showPrompt();
        break;
      }
      isProcessing = true;
      spinner.start('auth', 'Opening login prompt...');
      bridge.authenticate(args || undefined);
      break;

    case 'codex':
      // One-off prompt to Codex
      if (args) {
        sendPrompt(args, 'codex');
      } else {
        bridge?.setProvider('codex');
      }
      break;

    case 'claude':
      // One-off prompt to Claude
      if (args) {
        sendPrompt(args, 'claude');
      } else {
        bridge?.setProvider('claude');
      }
      break;

    case 'exit':
    case 'quit':
    case 'q':
      cleanup();
      process.exit(0);

    default:
      // Forward unknown commands to the provider
      if (backendReady && bridge) {
        sendPrompt(input);
      } else {
        console.log(formatError(`Unknown command: /${cmd}`));
        console.log(chalk.dim('Type /help for available commands'));
        showPrompt();
      }
      break;
  }
}

// Handle /provider command
function handleProviderCommand(args: string): void {
  if (!args) {
    // Show current provider
    console.log(formatInfo(`Current provider: ${activeProvider}`));
    if (capabilities) {
      console.log(chalk.dim(`  Available: ${capabilities.providers_available.join(', ')}`));
    }
    showPrompt();
    return;
  }

  const provider = args.toLowerCase();
  if (provider === 'codex' || provider === 'claude') {
    bridge?.setProvider(provider);
  } else {
    console.log(formatError(`Unknown provider: ${provider}`));
    console.log(chalk.dim('  Available: codex, claude'));
    showPrompt();
  }
}

// Print help
function printHelp(): void {
  console.log('');
  console.log(chalk.bold.cyan('  Commands:'));
  console.log(chalk.dim('  ─────────────────────────────────────'));
  console.log(`  ${chalk.yellow('/help')}          ${chalk.dim('Show this help message')}`);
  console.log(`  ${chalk.yellow('/voice')}         ${chalk.dim('Start voice capture')}`);
  console.log(`  ${chalk.yellow('/auto')}          ${chalk.dim('Toggle auto-voice mode')}`);
  console.log(`  ${chalk.yellow('/status')}        ${chalk.dim('Show backend status')}`);
  console.log(`  ${chalk.yellow('/provider')}      ${chalk.dim('Show/set active provider')}`);
  console.log(`  ${chalk.yellow('/auth [provider]')} ${chalk.dim('Login via provider CLI (/dev/tty)')}`);
  console.log(`  ${chalk.yellow('/codex <msg>')}   ${chalk.dim('Send to Codex (one-off or switch)')}`);
  console.log(`  ${chalk.yellow('/claude <msg>')}  ${chalk.dim('Send to Claude (one-off or switch)')}`);
  console.log(`  ${chalk.yellow('/clear')}         ${chalk.dim('Clear the screen')}`);
  console.log(`  ${chalk.yellow('/exit')}          ${chalk.dim('Exit the application')}`);
  console.log('');
  console.log(chalk.bold.cyan('  Shortcuts:'));
  console.log(chalk.dim('  ─────────────────────────────────────'));
  console.log(`  ${chalk.yellow('Ctrl+R')}         ${chalk.dim('Start voice capture')}`);
  console.log(`  ${chalk.yellow('Ctrl+V')}         ${chalk.dim('Toggle auto-voice mode')}`);
  console.log(`  ${chalk.yellow('Ctrl+C')}         ${chalk.dim('Cancel/Exit')}`);
  console.log('');
}

// Print status
function printStatus(): void {
  console.log('');
  console.log(chalk.bold.cyan('  Status:'));
  console.log(chalk.dim('  ─────────────────────────────────────'));
  console.log(`  Backend:      ${backendReady ? chalk.green('Connected') : chalk.red('Not connected')}`);
  console.log(`  Provider:     ${activeProvider === 'claude' ? chalk.magenta('claude') : chalk.cyan('codex')}`);
  console.log(`  Auto-voice:   ${autoVoiceMode ? chalk.green('Enabled') : chalk.dim('Disabled')}`);
  console.log(`  Processing:   ${isProcessing ? chalk.yellow('Yes') : chalk.dim('No')}`);

  if (capabilities) {
    console.log('');
    console.log(chalk.bold.cyan('  Capabilities:'));
    console.log(chalk.dim('  ─────────────────────────────────────'));
    console.log(`  Session:      ${chalk.dim(capabilities.session_id)}`);
    console.log(`  Microphone:   ${capabilities.mic_available ? chalk.green('Yes') : chalk.red('No')}`);
    if (capabilities.input_device) {
      console.log(`  Device:       ${chalk.dim(capabilities.input_device)}`);
    }
    console.log(`  Whisper:      ${capabilities.whisper_model_loaded ? chalk.green('Loaded') : chalk.yellow('Not loaded')}`);
    console.log(`  Providers:    ${chalk.dim(capabilities.providers_available.join(', '))}`);
    console.log(`  Working dir:  ${chalk.dim(capabilities.working_dir)}`);
  }
  console.log('');
}

// Send prompt to backend
async function sendPrompt(prompt: string, provider?: string): Promise<void> {
  if (!bridge || !backendReady) {
    console.log(formatError('Backend not ready'));
    showPrompt();
    return;
  }

  isProcessing = true;
  const targetProvider = provider || activeProvider;
  spinner.start('thinking', `${targetProvider === 'claude' ? 'Claude' : 'Codex'} is thinking...`);
  bridge.sendPrompt(prompt, provider);
}

// Handle events from Rust backend
function setupEventHandlers(): void {
  if (!bridge) return;

  bridge.on('capabilities', (event: CapabilitiesEvent) => {
    capabilities = event;
    activeProvider = event.active_provider;
    backendReady = true;
  });

  bridge.on('provider_changed', (event: { provider: string }) => {
    activeProvider = event.provider;
    console.log(formatSuccess(`Switched to ${event.provider}`));
    showPrompt();
  });

  bridge.on('provider_error', (event: { message: string }) => {
    console.log(formatError(event.message));
    showPrompt();
  });

  bridge.on('auth_start', (event: { provider: string }) => {
    spinner.stop();
    isProcessing = true;
    disableRawMode();
    console.log(formatInfo(`Login for ${event.provider} started. Follow the terminal prompts.`));
  });

  bridge.on('auth_end', (event: { provider: string; success: boolean; error?: string }) => {
    spinner.stop();
    isProcessing = false;

    if (event.success) {
      console.log(formatSuccess(`Login for ${event.provider} completed`));
    } else {
      console.log(formatError(`Login for ${event.provider} failed: ${event.error || 'unknown error'}`));
    }

    enableRawMode();
    showPrompt();
  });

  bridge.on('token', (event: { text: string }) => {
    spinner.stop();
    process.stdout.write(chalk.white(event.text));
  });

  bridge.on('voice_start', () => {
    spinner.update('Recording...');
  });

  bridge.on('voice_end', (event: { error?: string }) => {
    if (event.error) {
      spinner.fail(`Voice capture failed: ${event.error}`);
      isProcessing = false;
      showPrompt();
    }
  });

  bridge.on('transcript', (event: { text: string; duration_ms: number }) => {
    spinner.succeed(`Transcribed in ${event.duration_ms}ms`);
    console.log(voiceTranscript(event.text));
    sendPrompt(event.text);
  });

  bridge.on('job_start', (event: JobStartEvent) => {
    const providerName = event.provider === 'claude' ? 'Claude' : 'Codex';
    spinner.start('thinking', `${providerName} is thinking...`);
  });

  bridge.on('job_end', (event: JobEndEvent) => {
    spinner.stop();
    isProcessing = false;

    if (event.error) {
      console.log('\n' + formatError(event.error));
    } else {
      console.log(''); // newline after streaming output
    }

    if (autoVoiceMode && !event.error) {
      setTimeout(() => handleVoiceCapture(), 500);
    } else {
      showPrompt();
    }
  });

  bridge.on('status', (event: { message: string }) => {
    spinner.update(event.message);
  });

  bridge.on('error', (event: { message: string; recoverable: boolean }) => {
    spinner.fail(event.message);
    if (!event.recoverable) {
      cleanup();
      process.exit(1);
    }
    isProcessing = false;
    showPrompt();
  });

  bridge.on('log', (text: string) => {
    if (process.env.DEBUG) {
      console.log(chalk.dim(`[backend] ${text}`));
    }
  });

  bridge.on('exit', (code: number | null) => {
    backendReady = false;
    if (code !== 0 && code !== null) {
      console.log('\n' + formatError(`Backend exited with code ${code}`));
    }
  });
}

// Cleanup on exit
function cleanup(): void {
  spinner.stop();
  disableRawMode();
  rl?.close();
  bridge?.stop();
  console.log('\n' + chalk.dim('Goodbye!') + '\n');
}

function enableRawMode(): void {
  if (!process.stdin.isTTY || rawModeEnabled) {
    return;
  }

  process.stdin.setRawMode(true);
  process.stdin.resume();

  if (!rawInputHandler) {
    rawInputHandler = (key: Buffer) => {
      const keyStr = key.toString();
      const keyCode = key[0];

      // Ctrl+R - Voice capture (0x12 = 18)
      if (keyCode === 0x12 || keyStr === '\x12') {
        // Clear current line and start voice
        process.stdout.write('\r\x1b[K');
        handleVoiceCapture();
        return;
      }

      // Ctrl+V - Toggle auto-voice mode (0x16 = 22)
      if (keyCode === 0x16 || keyStr === '\x16') {
        process.stdout.write('\r\x1b[K');
        toggleAutoVoice();
        return;
      }

      // Ctrl+C
      if (keyStr === '\x03') {
        if (isProcessing) {
          bridge?.cancel();
          spinner.stop();
          isProcessing = false;
          console.log('\n' + formatInfo('Cancelled'));
          showPrompt();
        } else {
          cleanup();
          process.exit(0);
        }
        return;
      }

      // Ctrl+D - Exit
      if (keyStr === '\x04') {
        cleanup();
        process.exit(0);
      }

      // Enter key
      if (keyStr === '\r' || keyStr === '\n') {
        process.stdout.write('\n');
        handleInput(rawLineBuffer);
        rawLineBuffer = '';
        return;
      }

      // Backspace
      if (keyStr === '\x7f' || keyStr === '\b') {
        if (rawLineBuffer.length > 0) {
          rawLineBuffer = rawLineBuffer.slice(0, -1);
          process.stdout.write('\b \b');
        }
        return;
      }

      // Regular characters
      if (keyStr.charCodeAt(0) >= 32) {
        rawLineBuffer += keyStr;
        process.stdout.write(keyStr);
      }
    };
  }

  process.stdin.on('data', rawInputHandler);
  rawModeEnabled = true;
}

function disableRawMode(): void {
  if (!process.stdin.isTTY || !rawModeEnabled) {
    return;
  }

  if (rawInputHandler) {
    process.stdin.off('data', rawInputHandler);
  }

  process.stdin.setRawMode(false);
  rawModeEnabled = false;
  rawLineBuffer = '';
}

// Setup raw mode for Ctrl+R handling
function setupKeyHandlers(): void {
  enableRawMode();
}

// Main entry point
async function main(): Promise<void> {
  // Print welcome banner
  printBanner();
  printWelcome();

  // Try to start Rust backend first
  console.log('');
  bridge = new RustBridge();

  let backendStarted = false;
  try {
    console.log(chalk.dim('  Connecting to voice backend...'));
    capabilities = await bridge.start();
    activeProvider = capabilities.active_provider;
    backendReady = true;
    backendStarted = true;

    console.log(chalk.green('  ✓ Backend connected'));
    console.log(chalk.dim(`    Session: ${capabilities.session_id}`));
    console.log(chalk.dim(`    Provider: ${capabilities.active_provider}`));
    if (capabilities.mic_available) {
      console.log(chalk.dim(`    Microphone: ${capabilities.input_device || 'available'}`));
    }
    if (capabilities.whisper_model_loaded) {
      console.log(chalk.green('    Whisper: loaded'));
    } else {
      console.log(chalk.yellow('  ⚠ Whisper model not loaded - voice input disabled'));
      console.log(chalk.dim('    To enable voice, run: ./scripts/setup.sh models --base'));
    }
  } catch (err: any) {
    backendReady = false;
    console.log(chalk.yellow('  ⚠ Backend not available'));
    console.log(chalk.dim(`    ${err.message}`));
    console.log(chalk.dim('    To enable: cd rust_tui && cargo build --release'));
  }

  console.log('');

  // Setup event handlers if backend started
  if (backendStarted) {
    setupEventHandlers();
  }

  // Check if we can use raw mode for Ctrl+R
  if (process.stdin.isTTY) {
    setupKeyHandlers();
    showPrompt();
  } else {
    // Fallback to readline for non-TTY
    rl = readline.createInterface({
      input: process.stdin,
      output: process.stdout,
    });

    rl.on('line', async (line: string) => {
      await handleInput(line);
    });

    rl.on('SIGINT', () => {
      if (isProcessing) {
        bridge?.cancel();
        spinner.stop();
        isProcessing = false;
        console.log('\n' + formatInfo('Cancelled'));
        showPrompt();
      } else {
        cleanup();
        process.exit(0);
      }
    });

    rl.on('close', () => {
      cleanup();
      process.exit(0);
    });

    showPrompt();
  }
}

// Handle uncaught errors
process.on('uncaughtException', (err) => {
  console.error(formatError(`Error: ${err.message}`));
  cleanup();
  process.exit(1);
});

process.on('unhandledRejection', (reason) => {
  console.error(formatError(`Error: ${reason}`));
});

// Run
main().catch((err) => {
  console.error(formatError(`Fatal: ${err.message}`));
  process.exit(1);
});
