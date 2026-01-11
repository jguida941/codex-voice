import { spawn, ChildProcess } from 'child_process';
import { EventEmitter } from 'events';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// ============================================================================
// Event Types (Rust → TypeScript)
// ============================================================================

export interface IpcEvent {
  event: string;
  [key: string]: any;
}

/** Full capability information sent on startup */
export interface CapabilitiesEvent extends IpcEvent {
  event: 'capabilities';
  session_id: string;
  version: string;
  mic_available: boolean;
  input_device?: string;
  whisper_model_loaded: boolean;
  whisper_model_path?: string;
  python_fallback_allowed: boolean;
  providers_available: string[];
  active_provider: string;
  working_dir: string;
  codex_cmd: string;
  claude_cmd: string;
}

/** Provider changed successfully */
export interface ProviderChangedEvent extends IpcEvent {
  event: 'provider_changed';
  provider: string;
}

/** Error when trying to use provider-specific command on wrong provider */
export interface ProviderErrorEvent extends IpcEvent {
  event: 'provider_error';
  message: string;
}

/** Authentication flow started */
export interface AuthStartEvent extends IpcEvent {
  event: 'auth_start';
  provider: string;
}

/** Authentication flow ended */
export interface AuthEndEvent extends IpcEvent {
  event: 'auth_end';
  provider: string;
  success: boolean;
  error?: string;
}

/** Streaming token from provider */
export interface TokenEvent extends IpcEvent {
  event: 'token';
  text: string;
}

/** Voice capture started */
export interface VoiceStartEvent extends IpcEvent {
  event: 'voice_start';
}

/** Voice capture ended */
export interface VoiceEndEvent extends IpcEvent {
  event: 'voice_end';
  error?: string;
}

/** Transcript ready from voice capture */
export interface TranscriptEvent extends IpcEvent {
  event: 'transcript';
  text: string;
  duration_ms: number;
}

/** Provider job started */
export interface JobStartEvent extends IpcEvent {
  event: 'job_start';
  provider: string;
}

/** Provider job ended */
export interface JobEndEvent extends IpcEvent {
  event: 'job_end';
  provider: string;
  success: boolean;
  error?: string;
}

/** Status update */
export interface StatusEvent extends IpcEvent {
  event: 'status';
  message: string;
}

/** Error (recoverable or fatal) */
export interface ErrorEvent extends IpcEvent {
  event: 'error';
  message: string;
  recoverable: boolean;
}

// ============================================================================
// Command Types (TypeScript → Rust)
// ============================================================================

export interface IpcCommand {
  cmd: string;
  [key: string]: any;
}

/** Send a prompt to the active provider */
export interface SendPromptCommand extends IpcCommand {
  cmd: 'send_prompt';
  prompt: string;
  provider?: string; // Optional one-off provider override
}

/** Start voice capture */
export interface StartVoiceCommand extends IpcCommand {
  cmd: 'start_voice';
}

/** Cancel current operation */
export interface CancelCommand extends IpcCommand {
  cmd: 'cancel';
}

/** Set the active provider */
export interface SetProviderCommand extends IpcCommand {
  cmd: 'set_provider';
  provider: string;
}

/** Authenticate with provider */
export interface AuthCommand extends IpcCommand {
  cmd: 'auth';
  provider?: string;
}

/** Request capabilities (re-emit capabilities event) */
export interface GetCapabilitiesCommand extends IpcCommand {
  cmd: 'get_capabilities';
}

// ============================================================================
// Bridge Implementation
// ============================================================================

export class RustBridge extends EventEmitter {
  private process: ChildProcess | null = null;
  private buffer = '';
  private _ready = false;
  private _capabilities: CapabilitiesEvent | null = null;

  constructor(private rustBinaryPath?: string) {
    super();
  }

  async start(): Promise<CapabilitiesEvent> {
    // Find the Rust binary
    const binaryPath = this.rustBinaryPath || this.findRustBinary();

    // Check if binary exists
    const fs = await import('fs');
    if (!fs.existsSync(binaryPath)) {
      throw new Error(`Rust binary not found at: ${binaryPath}`);
    }

    return new Promise((resolve, reject) => {
      try {
        // Use CODEX_VOICE_CWD if set (allows start.sh to pass original dir)
        // Otherwise use current working directory
        const workingDir = process.env.CODEX_VOICE_CWD || process.cwd();

        this.process = spawn(binaryPath, ['--json-ipc'], {
          stdio: ['pipe', 'pipe', 'pipe'],
          env: { ...process.env },
          cwd: workingDir,
        });

        // Handle stdout (JSON events)
        this.process.stdout?.on('data', (data: Buffer) => {
          this.handleData(data.toString());
        });

        // Handle stderr (errors/logs)
        this.process.stderr?.on('data', (data: Buffer) => {
          const text = data.toString().trim();
          if (text) {
            this.emit('log', text);
          }
        });

        // Handle process exit
        this.process.on('exit', (code) => {
          this.emit('exit', code);
          this.process = null;
          this._ready = false;
        });

        // Handle spawn errors
        this.process.on('error', (err) => {
          this.process = null;
          reject(new Error(`Failed to start backend: ${err.message}`));
        });

        // Wait for capabilities event with timeout
        const readyTimeout = setTimeout(() => {
          if (this.process) {
            this.process.kill();
            this.process = null;
          }
          reject(new Error('Backend startup timeout (5s)'));
        }, 5000);

        this.once('capabilities', (event: CapabilitiesEvent) => {
          clearTimeout(readyTimeout);
          this._ready = true;
          this._capabilities = event;
          resolve(event);
        });
      } catch (err) {
        reject(err);
      }
    });
  }

  private findRustBinary(): string {
    // Look for the compiled Rust binary relative to this package
    const possiblePaths = [
      // Development: relative to ts_cli
      path.resolve(__dirname, '../../../rust_tui/target/release/rust_tui'),
      path.resolve(__dirname, '../../../rust_tui/target/debug/rust_tui'),
      // Installed: in PATH
      'rust_tui',
      // Alternative names
      'codex-voice-backend',
    ];

    return possiblePaths[0];
  }

  private handleData(data: string): void {
    this.buffer += data;

    // Process complete JSON lines
    const lines = this.buffer.split('\n');
    this.buffer = lines.pop() || ''; // Keep incomplete line in buffer

    for (const line of lines) {
      const trimmed = line.trim();
      if (!trimmed) continue;

      try {
        const event: IpcEvent = JSON.parse(trimmed);
        this.handleEvent(event);
      } catch (_err) {
        // Not valid JSON, emit as log
        this.emit('log', trimmed);
      }
    }
  }

  private handleEvent(event: IpcEvent): void {
    // Emit specific event type
    this.emit(event.event, event);

    // Also emit generic 'message' for all events
    this.emit('message', event);
  }

  send(command: IpcCommand): void {
    if (!this.process?.stdin) {
      throw new Error('Rust backend not running');
    }

    const json = JSON.stringify(command) + '\n';
    this.process.stdin.write(json);
  }

  /** Send a prompt to the active provider */
  sendPrompt(prompt: string, provider?: string): void {
    const cmd: SendPromptCommand = { cmd: 'send_prompt', prompt };
    if (provider) {
      cmd.provider = provider;
    }
    this.send(cmd);
  }

  /** Start voice capture */
  startVoice(): void {
    this.send({ cmd: 'start_voice' });
  }

  /** Cancel current operation */
  cancel(): void {
    this.send({ cmd: 'cancel' });
  }

  /** Set the active provider (codex or claude) */
  setProvider(provider: string): void {
    this.send({ cmd: 'set_provider', provider });
  }

  /** Authenticate with provider via /dev/tty login */
  authenticate(provider?: string): void {
    const cmd: AuthCommand = { cmd: 'auth' };
    if (provider) {
      cmd.provider = provider;
    }
    this.send(cmd);
  }

  /** Request capabilities */
  getCapabilities(): void {
    this.send({ cmd: 'get_capabilities' });
  }

  stop(): void {
    if (this.process) {
      this.process.kill();
      this.process = null;
      this._ready = false;
    }
  }

  isReady(): boolean {
    return this._ready;
  }

  get capabilities(): CapabilitiesEvent | null {
    return this._capabilities;
  }
}

// Singleton instance
let bridgeInstance: RustBridge | null = null;

export function getBridge(): RustBridge {
  if (!bridgeInstance) {
    bridgeInstance = new RustBridge();
  }
  return bridgeInstance;
}
