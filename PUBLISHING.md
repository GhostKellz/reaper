# 📦 Publishing Secure Tap Packages with Reaper

This guide walks you through securely publishing tap packages for Reaper with GPG signature verification.

## 1. Generate a GPG Key

If you don't have a GPG key:

```bash
gpg --full-generate-key
```

List your keys and copy the fingerprint:

```bash
gpg --list-keys --fingerprint
```

## 2. Sign Your PKGBUILD

Navigate to your package directory and run:

```bash
gpg --detach-sign --armor PKGBUILD
# This creates PKGBUILD.sig
```

## 3. Write publisher.toml

Create a `publisher.toml` in your tap root:

```toml
name = "Your Name"
gpg_key = "YOUR GPG FINGERPRINT"
email = "your@email.com"
verified = true
url = "https://your-site.com"
```

## 4. Commit and Push

Add `PKGBUILD`, `PKGBUILD.sig`, and `publisher.toml` to your tap repo and push.

## 5. Verify as a Publisher or User

- As a publisher: run `gpg --verify PKGBUILD.sig PKGBUILD` to check your signature.
- As a user: run `reap install <pkg>` and verify the CLI shows your publisher info and a green verification badge.

## Troubleshooting
- If users report GPG errors, ensure your key is uploaded to a public keyserver (e.g., `gpg --send-keys <keyid> --keyserver hkps://keys.openpgp.org`).
- Make sure publisher.toml is up to date and matches your GPG key.

---

For more details, see [README.md](README.md) and [DOCS.md](DOCS.md).
