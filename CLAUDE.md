# 🤖 Claude Memory - Reaper Project Context

## 📋 Current Project State

**Project**: Reaper v1.0.0 → v1.1.0 (AUR Helper + Build System in Zig)
**Repository**: `/data/projects/reaper` 
**Main Branch**: `main`
**Key Dependencies**: zsync v0.3.1, ghostnet v0.2.0, phantom TUI

## 🎯 Current Task: Version Bump Planning

### What We Just Analyzed
1. **zsync v0.3.1 Integration**: Currently using basic 4-thread runtime, not fully utilizing async capabilities
2. **Performance Gaps**: Sequential AUR API calls, blocking file I/O, manual threading for timeouts
3. **Architecture Discovery**: User has ghostnet (HTTP/1/2/3) + zquic (QUIC) + zsync (async runtime)

### Key Files Reviewed
- `build.zig.zon` - Dependencies (zsync v0.3.1)
- `TODO.md` - v1.1.0 roadmap with parallel operations goals
- `src/main.zig` - zsync runtime initialization (line 18)
- `src/async/subprocess.zig` - Manual threading instead of zsync async (line 41)
- `src/tui/tui.zig` - Well-designed TUI but not async-integrated

## 📝 Wishlist Documents Created

### 1. `ZSYNC_WISHLIST.md`
**Priority for v0.3.2:**
- ✅ Network Integration Layer (coordinate ghostnet/zquic with zsync)
- ✅ Async File I/O Operations (non-blocking cache/PKGBUILD handling)  
- ✅ Task Cancellation & Timeout Primitives (replace manual threading)

### 2. `PHANTOM_WISHLIST.md` 
**Priority for TUI enhancement:**
- ✅ Progress Components with Live Updates
- ✅ Background Task Integration with ZSync
- ✅ Live Data Streaming Components (logs, system monitoring)

### 3. `GHOSTNET_WISHLIST.md`
**Priority for v0.2.1:**
- ✅ HTTP Client Core API (concrete request/response interface)
- ✅ Connection Pool Management (AUR API efficiency)
- ✅ Download Progress & Streaming (package download UX)

## 🔧 Technical Architecture Insights

### Current Limitations
- **AsyncSubprocess**: Uses threads instead of zsync async (`src/async/subprocess.zig:41`)
- **AUR Backend**: Sequential HTTP calls without connection reuse (`backends/aur.zig:294`)
- **Build Manager**: Basic parallel flags but not async pipeline (`src/build/manager.zig:167`)
- **TUI**: Well-designed but blocks on operations, no background tasks

### Optimal Stack Integration
```
┌─────────────────┐
│   Reaper TUI    │ ← phantom (responsive, live updates)
├─────────────────┤
│   Async Tasks   │ ← zsync (coordinate all async operations)
├─────────────────┤
│   Network I/O   │ ← ghostnet (HTTP/1/2/3) + zquic (QUIC)
├─────────────────┤
│   File I/O      │ ← zsync async file ops
└─────────────────┘
```

## 📊 Performance Targets

### Current vs Target
- **AUR API Calls**: Sequential → 3-5x faster via HTTP/2+ multiplexing
- **Cache Operations**: Blocking → 2-3x faster via async file I/O
- **Build Pipeline**: Manual threads → Clean async with cancellation
- **Overall Goal**: 40-60% improvement in parallel workloads

### Version Bump Criteria
**For Reaper v1.1.0:**
- [ ] Update detection parity with yay/paru (Critical - TODO.md line 11)
- [ ] Async AUR pipeline using ghostnet + zsync
- [ ] Background update checking (non-blocking)
- [ ] Enhanced TUI with phantom integration
- [ ] Build cache async operations

## 🚀 Next Actions

### Immediate (Next Session)
1. **Implement zsync async file I/O** in cache operations
2. **Replace AsyncSubprocess threading** with zsync async primitives
3. **Add ghostnet HTTP client** for AUR API calls with connection pooling

### Dependencies to Update First
1. **ghostnet v0.2.1**: HTTP client core API, connection pooling
2. **zsync v0.3.2**: Network integration layer, async file I/O, task cancellation
3. **phantom**: Progress components, background task integration

### Testing Strategy
- **Performance benchmarks**: Before/after async conversion
- **AUR API compliance**: Rate limiting, connection reuse
- **User experience**: TUI responsiveness during operations

## 🔗 External Dependencies

- **zsync**: https://github.com/ghostkellz/zsync (main branch, v0.3.1)
- **ghostnet**: https://github.com/ghostkellz/ghostnet (main branch, v0.2.0)
- **phantom**: https://github.com/ghostkellz/phantom (main branch, TUI framework)

## 📁 Key Commands

### Build & Test
```bash
zig build                    # Build reaper
zig build test              # Run tests
reap --version              # Check current version
```

### Git Status
- **Modified**: `TODO.md`, `build.zig.zon`
- **Untracked**: `.zig-cache/`, `archive/`, wishlist files

## 💡 Architecture Decisions Made

1. **No HTTP duplication**: zsync coordinates existing ghostnet/zquic instead of implementing HTTP
2. **Keep existing TUI**: `src/tui/tui.zig` is well-designed, enhance with phantom components
3. **Async-first approach**: Replace all blocking operations with zsync async primitives
4. **Connection pooling priority**: Essential for AUR API performance gains

---

*Resume here: Focus on zsync async file I/O implementation and ghostnet HTTP client integration for immediate performance wins.*