# Publishing

Releases go to crates.io from [`.github/workflows/publish.yml`](.github/workflows/publish.yml).
No API token is stored in this repository. The workflow mints a short-lived one
over OIDC through crates.io Trusted Publishing.

One tag releases one crate.

## Before your first release

You need SSH commit signing set up. The workflow rejects unsigned tags.

```sh
git config --global gpg.format ssh
git config --global user.signingkey ~/.ssh/id_ed25519.pub
git config --global tag.gpgsign true
```

Add your public key to [`.github/allowed_signers`](.github/allowed_signers),
one line per maintainer:

```
you@example.org namespaces="git" ssh-ed25519 AAAA...
```

Without that file the workflow checks only that a signature exists, not whose
key made it. With it, `git verify-tag` runs for real.

## Releasing

1. Open a PR that bumps `version` in the crate's `Cargo.toml` and updates
   `Cargo.lock`. Get it reviewed and merged. The workflow refuses any tag that
   is not an ancestor of `main`, so this step is not optional.

2. Tag the merged commit and push:

   ```sh
   git switch main && git pull
   git tag -s rotortree-v0.18.0 -m 'rotortree 0.18.0'
   git push origin rotortree-v0.18.0
   ```

   `-s` signs the tag. `-a` alone fails the tag check.

3. The push starts the workflow. Watch it in the Actions tab.

4. **Another maintainer approves the deployment.** The `publish` job waits on
   the `crates-io` environment, which requires a reviewer who is not you. No
   token is minted until someone clicks approve.

5. Approve, and the crate goes out. The `attest` job then signs a provenance
   attestation.

Pushing a tag does not publish anything on its own. If nobody approves, nothing
reaches crates.io.

## What the workflow checks

| job | does |
|---|---|
| `verify` | tag matches `<crate>-v<semver>`, version matches `Cargo.toml`, tag is annotated and signed, commit is an ancestor of `main` |
| `gate` | clippy powerset, test powerset, `cargo audit`, semver check against the published version, `cargo package`, prints the packaged file list |
| `publish` | waits for approval, mints an OIDC token, `cargo publish --locked` |
| `attest` | confirms the registry checksum matches what we built, signs SLSA provenance |

Jobs after `verify` check out the commit SHA it resolved, not the tag, so
moving a tag mid-run cannot change what ships.

## Reviewing before you approve

You are the last human between a commit and every downstream build. Open the
`gate` job log and read:

- **the packaged file list**, under "Review package contents". Anything you do
  not recognise is a reason to stop.
- **any `build.rs` warning**. A build script runs on every machine that
  compiles the crate. The gate prints its full contents when one exists.
  Neither crate ships one today.
- **the semver check result**, if the crate has a published baseline.

Then approve in the Actions run.

## First publish of a crate name

crates.io cannot attach a Trusted Publisher to a name that does not exist yet.
Pending publishers are a deferred item in
[RFC 3691](https://rust-lang.github.io/rfcs/3691-trusted-publishing-cratesio.html),
not a feature. So the very first version of a new crate goes out by hand.

1. Create a token at <https://crates.io/settings/tokens> scoped as tightly as
   crates.io allows:
   - Crate scope: the exact crate name
   - Endpoint scope: `publish-new` only
   - Expiry: the shortest option offered

2. Publish from a clean checkout of the merged commit:

   ```sh
   git switch main && git pull && git status   # must be clean
   CARGO_REGISTRY_TOKEN=<token> cargo publish --locked -p sealring
   ```

   Pass the token inline as above. Do not run `cargo login`, which writes it to
   `~/.cargo/credentials.toml` and leaves it there.

3. Revoke the token immediately, before doing anything else.

4. Complete the crates.io setup below. Every later release uses the workflow.

This one publish skips the gate and the approval, so it is the weakest moment
in the crate's life. Keep it short.

## One-time setup

### crates.io, per crate

At `https://crates.io/crates/<crate>/settings`:

1. Add a GitHub Trusted Publisher:

   | field | value |
   |---|---|
   | Repository owner | `ethsystems` |
   | Repository name | `mono` |
   | Workflow filename | `publish.yml` |
   | Environment | `crates-io` |

   The environment field is the important one. It ties the OIDC claim to that
   one environment, so a token minted by any other job is rejected. The
   `attest` job relies on this: it holds `id-token: write` and still cannot
   publish, because it runs outside the environment.

2. Enable **Enforce Trusted Publishing**. API tokens stop working for the crate
   entirely.

3. Revoke every remaining publish token at <https://crates.io/settings/tokens>.

4. Confirm 2FA is on for every owner account.

### GitHub

1. Environment `crates-io` (Settings, Environments):
   - Required reviewers: `@rymnc`, `@oskarth`
   - Prevent self-review: on
   - Deployment branches and tags: selected refs, pattern `*-v*`
   - No secrets. The environment exists for the approval gate and the OIDC
     claim, not to hold a credential.

2. Tag ruleset (Settings, Rules) targeting `*-v*`: restrict creation to admins,
   block updates and deletions. A published tag then cannot be repointed.

3. Branch ruleset on `main`: require pull requests, require CI, require signed
   commits.

4. Set default `GITHUB_TOKEN` permissions to read-only.

## Verifying a release

crates.io stores no provenance and cargo verifies none, so this is manual:

```sh
curl -LO https://static.crates.io/crates/rotortree/rotortree-0.18.0.crate
gh attestation verify rotortree-0.18.0.crate \
  --repo ethsystems/mono \
  --signer-workflow ethsystems/mono/.github/workflows/publish.yml
```

`--signer-workflow` is load-bearing. Without it the check passes for any
attestation from this repository, including one signed by a workflow an
attacker added.

## When it fails

| symptom | cause |
|---|---|
| `tag '...' is not <crate>-v<semver>` | tag name is wrong, or the crate directory does not match |
| `tag says X but Cargo.toml declares Y` | version bump was not merged, or the tag is on the wrong commit |
| `'...' is a lightweight tag` | you used `git tag` without `-s` |
| `carries no signature` | signing is not configured, see above |
| `... is not an ancestor of origin/main` | tagged an unmerged commit |
| 403 from the token exchange | Trusted Publisher missing on crates.io, or its environment field does not say `crates-io` |
| job never starts | nobody approved the `crates-io` deployment |

A failed release leaves nothing behind except the tag. Delete it, fix the
cause, and tag again. Once `publish` succeeds the version is permanent, since
crates.io does not allow deletion. `cargo yank` marks a bad version unusable
for new dependents but does not remove it.
