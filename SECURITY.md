# 🛡️ Reaper Security Guide

Reaper prioritizes security in every aspect of package management. This guide explains the security features and best practices.

## 🎯 Security Philosophy

Reaper operates on a **"trust-first"** philosophy where every package is analyzed for security risks before installation. No package is installed without the user being informed of its security status.

## 🔒 Trust Scoring System

### Trust Levels
Every package receives a trust score (0-10) and corresponding badge:

- **🛡️ TRUSTED (8.0-10.0)**
  - Valid PGP signatures
  - Verified publisher
  - High community trust
  - No security flags

- **✅ VERIFIED (6.0-7.9)**
  - Some verification present
  - Good community standing
  - Minor security considerations

- **⚠️ CAUTION (4.0-5.9)**
  - Mixed security indicators
  - Review recommended
  - Moderate risk factors

- **🚨 RISKY (2.0-3.9)**
  - Multiple security concerns
  - Careful review required
  - High-risk operations detected

- **❌ UNSAFE (0.0-1.9)**
  - Serious security issues
  - Installation not recommended
  - Manual override required

### Trust Score Calculation

The trust score considers multiple factors:

1. **PGP Signature Verification** (+2.0 points)
   - Valid signature from trusted key
   - Key in trusted keyring
   - Signature not expired

2. **Publisher Verification** (+1.5 points)
   - Known maintainer with good reputation
   - Verified email and identity
   - Consistent maintenance history

3. **Community Trust** (+1.0 point max)
   - AUR vote count (0.01 points per vote, max 1.0)
   - Package popularity score
   - Community feedback

4. **Maintainer Reputation** (±0.5 points)
   - Historical reliability
   - Response to security issues
   - Code quality track record

5. **Security Flags** (-0.5 points each)
   - Suspicious PKGBUILD patterns
   - Network access requirements
   - System-level permissions
   - Known vulnerabilities

## 🔍 Security Analysis Features

### PKGBUILD Security Scanning

Reaper analyzes PKGBUILDs for suspicious patterns:

```bash
# Patterns that trigger security flags:
- Network operations: curl, wget, git clone
- System access: sudo, chmod +x
- File manipulation: rm -rf, dd if=
- Code execution: eval, exec
- Temporary files: mktemp
```

### Signature Verification

```bash
# Check package signatures
reap trust score firefox
# Shows signature status and key details

# Verify all installed packages
reap trust scan
# Comprehensive signature verification
```

### Publisher Verification

For tap repositories:
- GPG key verification
- Publisher identity validation
- Repository authenticity checks

## 🏗️ Profile-Based Security

### Security Profiles

#### Strict Mode (Developer Profile)
```toml
strict_signatures = true
auto_resolve_deps = true
backend_order = ["tap", "aur", "flatpak"]
```
- Requires valid signatures for all packages
- Blocks installation of untrusted packages
- Comprehensive dependency verification

#### Moderate Mode (Default)
```toml
strict_signatures = false
auto_resolve_deps = true
```
- Warns about security issues but allows installation
- User confirmation for risky packages
- Balanced security and usability

#### Permissive Mode (Gaming Profile)
```toml
strict_signatures = false
fast_mode = true
```
- Minimal security checks for compatibility
- Prioritizes functionality over strict security
- Still shows trust scores for awareness

### Profile Security Commands

```bash
# Create secure profile
reap profile create secure --template developer
reap profile edit secure --strict-signatures

# Switch to secure mode
reap profile switch secure

# Verify current security settings
reap profile show secure
```

## 🔐 Best Practices

### Package Installation
1. **Always review trust scores** before installing packages
2. **Check PKGBUILD diffs** for unfamiliar packages
3. **Use strict mode** for critical systems
4. **Verify signatures** manually for high-risk packages

```bash
# Safe installation workflow
reap trust score suspicious-package   # Check trust first
reap aur fetch suspicious-package     # Review PKGBUILD
reap install suspicious-package --diff # See changes before install
```

### Profile Management
1. **Use appropriate profiles** for different contexts
2. **Keep secure profiles** for sensitive work
3. **Regular profile audits** to ensure proper settings
4. **Backup profile configurations**

### System Maintenance
1. **Regular trust scans** to identify new security issues
2. **Update trust database** to get latest security information
3. **Monitor security advisories** for installed packages
4. **Clean up untrusted packages** periodically

```bash
# Security maintenance routine
reap trust scan                    # Full system scan
reap trust update                  # Update security database
reap profile show $(reap profile current) # Verify profile security
```

## 🚨 Security Alerts

### Automatic Warnings

Reaper automatically warns about:
- Packages with failing signatures
- Known vulnerable packages
- Suspicious PKGBUILD content
- Packages requesting excessive permissions

### User Confirmations

Critical operations require explicit confirmation:
- Installing packages with trust score < 4.0
- PKGBUILD editing for any package
- Overriding signature verification
- Installing packages with security flags

## 🔧 Advanced Security Features

### Audit Mode
```bash
# Comprehensive security audit
reap trust scan --verbose
# Shows detailed security analysis for all packages

# Audit specific package
reap aur deps firefox --conflicts
# Check for dependency and conflict issues
```

### Security Reporting
```bash
# Generate security report
reap trust stats
# Overview of system security status

# Export security data
reap trust export --format json
# Machine-readable security information
```

### Custom Security Policies

Advanced users can customize security policies:

```rust
// Custom trust calculation
impl TrustEngine {
    fn custom_security_policy(&self, pkg: &str) -> SecurityPolicy {
        // Implement custom security rules
    }
}
```

## 🔒 Cryptographic Security

### GPG Integration
- **Key verification**: All signatures checked against trusted keyring
- **Key expiration**: Expired keys trigger security warnings
- **Key trust levels**: Different trust levels for different keys
- **Automatic key refresh**: Regular updates to GPG keyring

### Secure Communication
- **HTTPS only**: All network communications encrypted
- **Certificate validation**: Strict certificate checking
- **Connection security**: Protection against man-in-the-middle attacks

## 📊 Security Monitoring

### Real-time Monitoring
- **Live trust updates**: Security scores updated in real-time
- **Automatic alerts**: Notifications for security issues
- **Progress tracking**: Security verification during installations

### Security Metrics
- **Trust distribution**: Overview of package trust levels
- **Risk assessment**: Identification of high-risk packages
- **Trend analysis**: Security improvements over time

## 🛠️ Emergency Procedures

### Compromised Package Response
1. **Immediate isolation**: Stop installation if possible
2. **Risk assessment**: Evaluate potential damage
3. **System verification**: Check for unauthorized changes
4. **Package removal**: Safe removal of compromised package
5. **Security scan**: Full system security audit

### Recovery Procedures
1. **Profile reset**: Restore to secure profile
2. **Trust database rebuild**: Refresh all trust information
3. **Signature re-verification**: Re-check all signatures
4. **System audit**: Comprehensive security review

This security framework ensures that Reaper users can confidently manage packages while maintaining strong security posture.