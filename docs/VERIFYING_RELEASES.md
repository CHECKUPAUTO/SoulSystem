# Verifying a SoulSystem release

Every release artifact is signed with [Sigstore](https://www.sigstore.dev/)
using cosign in **keyless** mode. There is no public key to fetch and no
private key anywhere: the signer's identity is the GitHub Actions workflow
that produced the build, recorded in a short-lived certificate and logged in
the public Rekor transparency log.

## What you need

```
cosign        # https://docs.sigstore.dev/cosign/installation/
```

## Verify a binary archive

Download the archive and its `.bundle` from the release page, then:

```sh
cosign verify-blob \
  --bundle soulsystem-x86_64-unknown-linux-gnu.tar.gz.bundle \
  --certificate-identity-regexp \
    '^https://github.com/Memorithm/SoulSystem/\.github/workflows/release\.yml@refs/tags/' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  soulsystem-x86_64-unknown-linux-gnu.tar.gz
```

Success prints `Verified OK`. Anything else means **do not install it**.

The two `--certificate-*` flags are the whole point and are not optional
decoration:

- `--certificate-identity-regexp` says *who* signed it. Without it, cosign
  accepts a valid signature from **anybody** — including an attacker who
  signed their own artifact with their own Sigstore identity.
- `--certificate-oidc-issuer` says which identity provider vouched for that
  identity.

A verification command missing either flag proves the file was signed by
someone, which is not the question you are asking.

## Verify the SBOM

Same shape:

```sh
cosign verify-blob \
  --bundle soulsystem-sbom.tar.gz.bundle \
  --certificate-identity-regexp \
    '^https://github.com/Memorithm/SoulSystem/\.github/workflows/release\.yml@refs/tags/' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  soulsystem-sbom.tar.gz
```

## Verify the checksum too, if you want belt and braces

Each archive ships a `.sha256`, and that file is itself signed. Verify the
signature on the `.sha256` first, then check the archive against it — that
order matters, since an unverified checksum file tells you nothing.

```sh
cosign verify-blob --bundle soulsystem-…​.tar.gz.sha256.bundle \
  --certificate-identity-regexp '…' --certificate-oidc-issuer '…' \
  soulsystem-…​.tar.gz.sha256
shasum -a 256 -c soulsystem-…​.tar.gz.sha256
```

## What the release workflow proves about itself

Signing that is never verified drifts silently: the day the identity or the
issuer changes, releases keep shipping with signatures nobody can check. So
the workflow does not just sign — on every release it:

1. signs each artifact,
2. verifies the signature it just produced,
3. copies the artifact, flips one byte, and requires that verification
   **fails**.

If the tampered copy verifies, the signature is not binding the content and
the job aborts rather than publishing. That third step is what makes this a
test rather than a ceremony — without it, step 2 only proves that we verified
what we just signed.

## Relationship to `src/code_signing.rs`

These are two different mechanisms with two different jobs, and conflating
them would be a mistake:

| | `src/code_signing.rs` | this document |
|---|---|---|
| Protects | code the agent writes for **itself** at runtime | artifacts **distributed** to users |
| Trust root | `AuthorizedKeys`, a local allowlist | Sigstore, a public transparency log |
| Question | "may this process load this code?" | "did the SoulSystem project build this file?" |

`code_signing.rs` gates self-modification: an autonomous agent that can write
and load its own code needs a local authority saying which code is
permitted. Sigstore cannot answer that — it attests provenance to the outside
world, not authority inside a running process.

They are deliberately not unified. What *would* be a defect is a second
signing system that nothing uses; both of these have a caller, and the
comparison is written here so a future reader does not "unify" them and lose
one of the two properties.
