# Codex Voice Optimization Plan

## Current Issues
- **5-6 second latency** per voice command
- **New Codex process spawned** for each prompt (major overhead)
- **No session persistence** - loses context between commands
- **Fixed recording duration** - wastes time when user stops speaking early
- **TUI disruption** from external command output (now fixed)

## Implementation Roadmap

### Phase 1: Stabilize & Measure (Immediate)
**Goal: Fix fundamental architecture issues and understand bottlenecks**

1. **Persistent Codex PTY Session**
   - [ ] Implement long-lived PTY session manager in Python
   - [ ] Keep Codex process alive across multiple prompts
   - [ ] Maintain conversation context without re-initialization
   - [ ] Handle session recovery on crashes

2. **Rust TUI Integration**
   - [ ] Connect TUI to persistent session via IPC/socket
   - [ ] Option 1: Python daemon with Unix socket communication
   - [ ] Option 2: Direct PTY handling in Rust with `portable-pty`
   - [ ] Bidirectional streaming for real-time output

3. **Performance Instrumentation**
   - [ ] Add timing for each stage:
     - Audio capture duration
     - Whisper transcription time
     - Codex response latency
     - PTY communication overhead
   - [ ] Display metrics in TUI status bar
   - [ ] Log detailed timing to identify bottlenecks

### Phase 2: Optimize User Experience (Next Sprint)
**Goal: Reduce perceived latency from 5-6s to <2s**

1. **Voice Activity Detection (VAD)**
   - [ ] Replace fixed `--seconds` with dynamic recording
   - [ ] Implement WebRTC VAD or `py-webrtcvad`
   - [ ] Stop recording after 1-2s of silence
   - [ ] Visual feedback during active speech detection

2. **Whisper Optimization**
   - [ ] **Option A: Whisper.cpp with smaller models**
     - Use `tiny` or `base` models for <1s transcription
     - Keep model loaded in memory
   - [ ] **Option B: Whisper API service**
     - Run local Whisper server (stays warm)
     - Stream audio chunks for faster processing
   - [ ] **Option C: OpenAI Whisper API**
     - For users with good internet (parallel processing)

3. **Session Warming**
   - [ ] Pre-initialize Codex on TUI startup
   - [ ] Maintain warm connection pool
   - [ ] Cache common responses/completions
   - [ ] Implement session state management

4. **Streaming Responses**
   - [ ] Stream Codex output token-by-token to TUI
   - [ ] Show partial results immediately
   - [ ] Progressive rendering reduces perceived wait

### Phase 3: Architecture Refinement (Future)
**Goal: Consolidate to optimal tech stack based on metrics**

1. **Language Consolidation Decision Tree**
   ```
   If Python overhead > 200ms:
     → Migrate control plane to Rust
   If audio processing is bottleneck:
     → Consider native C++ with whisper.cpp
   If everything else is optimized:
     → Stay with Python+Rust hybrid
   ```

2. **Potential Full Rust Migration**
   - [ ] Native PTY handling with `portable-pty`
   - [ ] Direct whisper.cpp FFI bindings
   - [ ] Async/await for concurrent operations
   - [ ] Single binary distribution

3. **Alternative Architectures**
   - [ ] Client-server model with gRPC
   - [ ] Background service with system tray
   - [ ] VS Code / IDE extensions

### Phase 4: Production Ready (Long-term)
**Goal: Professional tool for widespread adoption**

1. **Operational Excellence**
   - [ ] One-line installation script
   - [ ] Auto-download models and dependencies
   - [ ] Cross-platform installers (brew, apt, winget)
   - [ ] Automatic updates

2. **Testing & Quality**
   - [ ] Unit tests for each component
   - [ ] Integration tests with mocked services
   - [ ] Stubbed ffmpeg/whisper/codex for CI
   - [ ] Performance regression tests
   - [ ] End-to-end voice flow tests

3. **Documentation**
   - [ ] Environment setup guide
   - [ ] Troubleshooting handbook
   - [ ] Performance tuning guide
   - [ ] API documentation for extensions

## Quick Wins (Do Now)

### Immediate Performance Gains
1. **Use `whisper` `base` model instead of `small`** (2-3x faster)
2. **Reduce recording time to 5 seconds default**
3. **Add `--no-warnings` flag to suppress Whisper warnings**
4. **Use `CODEX_NONINTERACTIVE=1` for faster responses**

### Configuration Optimizations
```bash
# ~/.config/codex-voice/config.toml
[performance]
whisper_model = "base"  # Faster than "small"
recording_seconds = 5   # Reduce from 8
parallel_transcription = true
cache_transcripts = true

[codex]
keep_alive = true
session_timeout = 300  # 5 minutes
streaming = true
```

## Metrics & Success Criteria

### Target Latencies
| Stage | Current | Target | Stretch Goal |
|-------|---------|--------|--------------|
| Audio Capture | 8s fixed | 2-4s VAD | <2s VAD |
| Transcription | 2-3s | <1s | <500ms |
| Codex Response | 1-2s | <500ms | <200ms |
| **Total Loop** | **5-6s** | **<2s** | **<1s** |

### Key Performance Indicators
- Time to first token displayed
- Total round-trip time
- Session persistence uptime
- Memory usage over time
- CPU usage during idle

## Implementation Priority

### Week 1-2: Core Stability
- Persistent PTY session (Python)
- Basic instrumentation
- Fix remaining TUI issues

### Week 3-4: Speed Optimization
- VAD implementation
- Whisper optimization
- Streaming responses

### Month 2: Production Hardening
- Full test suite
- Cross-platform testing
- Documentation
- Distribution packages

## Technical Decisions

### Why Python + Rust (for now)
- **Python**: Fast prototyping, rich ecosystem (VAD, audio libs)
- **Rust**: Reliable TUI, memory safety, distribution
- **Migration Path**: Clear metrics before rewriting

### Why Not Go/C++ (yet)
- **Go**: No clear advantage over Python for glue code
- **C++**: Only if embedding whisper.cpp becomes critical
- **Overhead**: Rewriting delays feature development

## Alternative Approach: Service Architecture

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│   Rust TUI   │────▶│ Python Core  │────▶│ Codex PTY    │
│   (Client)   │     │   (Service)  │     │  (Session)   │
└──────────────┘     └──────────────┘     └──────────────┘
                            │
                     ┌──────▼──────┐
                     │   Whisper    │
                     │   Service    │
                     └──────────────┘
```

Benefits:
- TUI can restart without losing session
- Multiple clients (TUI, CLI, IDE)
- Better resource management
- Easier testing

## Conclusion

**Immediate Focus**: Persistent sessions and VAD will provide the biggest wins. The 5-6 second latency is primarily from process spawning and fixed recording time, not language overhead.

**Recommendation**: Stay with Python+Rust, implement persistent PTY and VAD first. Only consider language migration after proving Python is the bottleneck with hard metrics.

**Next Steps**:
1. Implement persistent PTY session manager
2. Add performance instrumentation
3. Switch to VAD-based recording
4. Test and measure improvements
5. Reassess architecture based on data