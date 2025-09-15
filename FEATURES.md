# 🚀 Reaper Features Documentation

This document provides comprehensive information about all Reaper v0.5.0 features and capabilities.

## 🛡️ Trust & Security Engine

### Trust Scoring System
Reaper analyzes every package for security risks and assigns a trust score (0-10):

```bash
reap trust score firefox
# Output: firefox 🛡️ TRUSTED (Score: 8.5/10)
```

**Trust Levels:**
- **🛡️ TRUSTED** (8.0-10.0): High confidence, verified signatures, known publisher
- **✅ VERIFIED** (6.0-7.9): Good package, some verification
- **⚠️ CAUTION** (4.0-5.9): Moderate risk, review recommended  
- **🚨 RISKY** (2.0-3.9): High risk, careful review required
- **❌ UNSAFE** (0.0-1.9): Very high risk, avoid installation

### Security Analysis Features
- **PGP Signature Verification**: Checks package signatures against trusted keys
- **PKGBUILD Security Scanning**: Analyzes build scripts for suspicious patterns
- **Publisher Verification**: Validates package maintainer credentials
- **Dependency Vulnerability Scanning**: Checks for known security issues
- **File Integrity Checks**: Validates package contents

### Security Commands
```bash
reap trust score <package>     # Analyze single package
reap trust scan                # Scan all installed packages  
reap trust stats               # Show system trust statistics
reap trust update              # Update trust database
```

## ⭐ Community Rating System

### AUR Integration
Reaper integrates with AUR to show real community data:
- **Vote counts**: Direct from AUR database
- **Popularity scores**: AUR popularity metrics
- **Maintainer info**: Package maintainer details

### User Ratings
Rate packages with 1-5 stars and optional comments:

```bash
reap rate firefox 5 "Excellent browser, very stable"
reap rate vim 4 "Great editor but steep learning curve"
reap rate htop 5
```

### Rating Display
Ratings appear in search results and TUI:
```
firefox 🛡️ TRUSTED ⭐⭐⭐⭐⭐ 4.8/5.0 (1,250 votes) - Web browser
```

## 👤 Multi-Profile Management

### Profile System
Switch between different package management configurations:

```bash
# Create profiles
reap profile create dev --template developer
reap profile create gaming --template gaming  
reap profile create minimal --template minimal

# Switch profiles
reap profile switch dev
reap profile list
reap profile show dev
```

### Profile Templates

#### Developer Profile
- **Backend order**: tap → aur → flatpak
- **Auto-install deps**: base-devel, git, rust, nodejs, python
- **Security**: Strict signature verification
- **Performance**: 8 parallel jobs

#### Gaming Profile  
- **Backend order**: flatpak → aur → chaotic-aur
- **Auto-install deps**: steam, lutris, wine, gamemode
- **Security**: Relaxed for compatibility
- **Performance**: 6 parallel jobs, fast mode

#### Minimal Profile
- **Backend order**: pacman → aur only
- **Auto-install deps**: None
- **Security**: Basic verification
- **Performance**: 2 parallel jobs, conservative

### Profile Settings
Each profile can customize:
- Backend priority order
- Security strictness level
- Parallel job count
- Auto-installed dependencies
- Fast mode vs verification mode

## 🔧 Advanced AUR Operations

### PKGBUILD Management
```bash
# Fetch and analyze PKGBUILD
reap aur fetch firefox
# Downloads, parses, and caches PKGBUILD with dependency info

# Interactive editing
reap aur edit custom-package  
# Opens PKGBUILD in $EDITOR with safety confirmations

# Dependency analysis
reap aur deps firefox --conflicts
# Checks for circular dependencies and conflicts
```

### Conflict Detection
Reaper performs advanced analysis to detect:
- **Package conflicts**: Direct package name conflicts
- **File conflicts**: Files that would overwrite each other
- **Version conflicts**: Incompatible version requirements
- **Circular dependencies**: Dependency loops that can't be resolved

### PKGBUILD Parsing
Extracts structured information:
- Package metadata (name, version, description)
- Dependencies and make dependencies
- Conflict and provides lists
- Source files and integrity checks
- Build instructions analysis

## 📋 Enhanced Interactive TUI

### Five-Tab Interface

#### 🔍 Search Tab
- **Trust-aware search**: Results show trust badges and ratings
- **Real-time scoring**: Trust analysis as you search
- **Rating integration**: Community and user ratings displayed
- **Interactive hotkeys**: 
  - `t`: Show trust details
  - `r`: Rate selected package
  - `d`: Show PKGBUILD diff
  - `TAB`: Toggle details panel

#### 📦 Queue Tab
- **Installation queue**: Packages waiting for installation
- **Live build progress**: Real-time makepkg output and progress bars
- **Dependency visualization**: Show what's being built and why
- **Build monitoring**: ETA, current stage, parallel builds

#### 📋 Log Tab  
- **Real-time activity log**: All reaper operations
- **Colored output**: Error, warning, success indicators
- **Scrollable history**: Full session history
- **Filter capabilities**: Focus on specific operations

#### 👤 Profiles Tab
- **Profile switching**: Quick profile changes
- **Profile details**: View current profile configuration
- **Profile creation**: Create new profiles interactively
- **Settings overview**: Backend order, security settings

#### 🖥️ System Tab
- **Package statistics**: Installed package breakdown by source
- **System health**: Outdated packages, security alerts
- **Performance metrics**: Install times, cache usage
- **Visual charts**: Package distribution graphs

### Interactive Features
- **Live monitoring**: Real-time updates during operations
- **Progress tracking**: Visual progress bars for builds
- **Trust integration**: Security scores in all package lists
- **Rating display**: Star ratings throughout interface

## 💬 Interactive Prompts & Safety

### Confirmation System
Reaper uses smart confirmations for safety:

```bash
# Package removal confirmation
reap remove firefox
# Shows: "🗑️ The following packages will be REMOVED:"
#        "  - firefox"
#        "Do you want to continue? [y/N]:"

# PKGBUILD editing safety
reap aur edit risky-package
# Shows: "📝 PKGBUILD for risky-package is about to be opened"
#        "⚠️ Only edit if you understand the implications!"
#        "Do you want to edit the PKGBUILD? [y/N]:"
```

### Diff Viewer
View changes before installation:

```bash
reap install firefox --diff
# Shows colored diff of PKGBUILD changes
# 🔴 - removed lines
# 🟢 + added lines  
# Confirms before proceeding
```

### Interactive Selection
Smart menus for choices:
```bash
reap search browser
# Multiple results? Interactive selection menu
# "Select package to install:"
# "  1: firefox"
# "  2: chromium" 
# "  3: brave-bin"
# "Enter your choice (1-3):"
```

## 🚀 Intelligent Dependency Resolution

### Advanced Algorithm
- **Circular dependency detection**: Identifies and suggests breaking cycles
- **Conflict prediction**: Warns about conflicts before installation
- **Version constraint solving**: Resolves complex version requirements
- **Optimal installation order**: Minimizes build times and dependencies

### Conflict Resolution
When conflicts are detected:
```bash
reap aur deps complex-package --conflicts
# Output: "⚠️ 2 conflicts detected:"
#         "  • Package conflict: old-package vs new-package"
#         "  • File conflict: /usr/bin/tool"
#         "Suggestions:"
#         "  - Remove old-package first"
#         "  - Use --force to override file conflicts"
```

## 📊 Real-time Analytics & Monitoring

### Build Progress Tracking
- **Live makepkg output**: Stream build logs in real-time
- **Progress estimation**: ETA based on package size and system performance
- **Parallel build monitoring**: Track multiple simultaneous builds
- **Resource usage**: CPU, memory, disk usage during builds

### System Statistics
- **Package distribution**: Visual breakdown of package sources
- **Performance metrics**: Installation times, success rates
- **Security overview**: Trust score distribution
- **Update monitoring**: Track outdated packages

### Cache Management
- **Intelligent caching**: PKGBUILD and metadata caching
- **Cache analytics**: Size, hit rates, cleanup opportunities
- **Build cache**: Reuse build artifacts when possible

## 🔗 Integration Features

### Profile-Aware Operations
Every operation respects the active profile:
- **Backend selection**: Uses profile's preferred backend order
- **Security settings**: Applies profile's security requirements
- **Performance tuning**: Uses profile's parallel job settings

### Trust-Guided Decisions
Security considerations throughout:
- **Installation filtering**: Warn or block untrusted packages
- **Audit trail**: Track all security decisions
- **Security updates**: Prioritize packages with security implications

This comprehensive feature set makes Reaper a powerful, secure, and user-friendly package manager for Arch Linux users who want advanced functionality without sacrificing safety or usability.