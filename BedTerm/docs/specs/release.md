# BedTerm Release — Implementation Spec

**PRD:** `prd/bedterm/release.xml`
**Goal:** Ship BedTerm v1.0 to the App Store on an individual Apple Developer Program account.

This is the actionable checklist. Work through the phases in order. Each row maps to one PRD rule and one concrete action you (or Claude) can take. Tick rows as `✅` here when done; flip the matching `<rule status="❌">` in the PRD to `✅` in the same PR.

## Locked-in decisions (do not relitigate without a PRD edit)

| Field | Value |
|---|---|
| Apple Developer Program | Individual enrollment |
| Bundle identifier | `dev.yjmeqt.bedterm` (replaces `com.applovin.yi.bedterm`) |
| App Store name | BedTerm |
| Subtitle (default — confirm before submitting) | A terminal you can use in bed |
| Voice for promo + description | First-person founder voice |
| Pricing | Free, no IAP |
| Categories | Primary: Developer Tools. Secondary: Utilities. |
| Privacy posture | Data Not Collected |
| Encryption | `ITSAppUsesNonExemptEncryption=false` (already set) |
| Privacy/Support hosting | GitHub Pages from this repo's `docs/` |
| EULA | Default Apple EULA |
| iPad | Supported (existing target) |
| Localized listing locales | en, zh-Hans, zh-Hant, ja, ko, ru |

## Architecture notes

- No application-code change is required for this work *except*: the bundle ID rename in `project.pbxproj`, adding `BedTerm/PrivacyInfo.xcprivacy`, and possibly swapping the marketing-icon asset. All other deliverables are metadata, hosted documents, or App Store Connect configuration.
- Sources of truth: the PRD (rules + bugs), this spec (action checklist + architecture), and `bedterm/docs/release/app-store-listing.md` (copy in all locales).

## Phase 1 — Account & Identity (PRD R1, R2)

| # | Action | PRD rule | Status |
|---|---|---|---|
| 1.1 | Enroll your Apple ID in the Apple Developer Program as an individual. Confirm the team appears in [developer.apple.com/account](https://developer.apple.com/account). | R1.enrollment_individual | ❌ |
| 1.2 | Verify 2FA is on for the Apple ID. | R1.two_factor | ❌ |
| 1.3 | Sign the Program License Agreement in [App Store Connect → Agreements](https://appstoreconnect.apple.com/agreements). | R1.agreements_signed | ❌ |
| 1.4 | Register App ID `dev.yjmeqt.bedterm` (explicit, no wildcards) in Certificates, Identifiers & Profiles → Identifiers. | R2.bundle_id_registered | ❌ |
| 1.5 | In `BedTerm.xcodeproj/project.pbxproj`, change `PRODUCT_BUNDLE_IDENTIFIER` for the three targets: `com.applovin.yi.bedterm` → `dev.yjmeqt.bedterm`, plus `.tests` and `.uitests` suffixed siblings. | R2.bundle_id_registered | ❌ |
| 1.6 | In Xcode → Signing & Capabilities, switch Team to your individual team. Verify Automatic signing succeeds. | R1.team_visible_in_xcode | ❌ |
| 1.7 | Verify Debug build still runs on simulator and on your physical iPhone. | R1.team_visible_in_xcode | ❌ |
| 1.8 | Create the app record in App Store Connect: name "BedTerm", bundle `dev.yjmeqt.bedterm`, primary language English, SKU `bedterm-001`. | R2.app_record_created | ❌ |
| 1.9 | Set primary category = Developer Tools, secondary = Utilities. | R2.category_chosen | ❌ |

## Phase 2 — Hosted Legal Documents (PRD R5)

| # | Action | PRD rule | Status |
|---|---|---|---|
| 2.1 | Write `docs/legal/privacy.md`. Cover: no data collection, no analytics, SSH credentials stored in iOS Keychain (with optional iCloud Keychain sync if user enables), no third-party SDKs that phone home, contact email. | R5.privacy_policy_url | ❌ |
| 2.2 | Write `docs/legal/support.md`. Contact email, link to GitHub issues, common-questions list (how to use SSH keys, where credentials are stored, troubleshooting). | R5.support_url | ❌ |
| 2.3 | Enable GitHub Pages in repo settings → Pages: source = `main` branch, folder `/docs`. Verify the two pages are reachable. | R5.privacy_policy_url, R5.support_url | ❌ |
| 2.4 | (Optional) Add `docs/index.md` as a one-page marketing front for the repo. | R5.marketing_url | ❌ |
| 2.5 | Paste privacy URL and support URL into App Store Connect → App Information. | R5.privacy_policy_url, R5.support_url | ❌ |
| 2.6 | Default Apple EULA — no action; leave the custom-EULA field blank. | R5.eula | ❌ |
| 2.7 | Complete the App Privacy questionnaire in App Store Connect with the "Data Not Collected" path. Verify every answer matches `docs/legal/privacy.md`. | R5.privacy_questionnaire | ❌ |
| 2.8 | Add `BedTerm/PrivacyInfo.xcprivacy`. Audit the codebase for required-reason API usage and declare each (likely just `NSPrivacyAccessedAPICategoryUserDefaults` with reason `CA92.1`). | R5.privacy_manifest | ❌ |

## Phase 3 — Listing Content (PRD R3, R4)

| # | Action | PRD rule | Status |
|---|---|---|---|
| 3.1 | Create `bedterm/docs/release/app-store-listing.md` with one section per locale (en, zh-Hans, zh-Hant, ja, ko, ru). | (umbrella) | ❌ |
| 3.2 | English name field: "BedTerm". Reserve in ASC. | R3.app_name | ❌ |
| 3.3 | English subtitle: "A terminal you can use in bed" (30 chars). Confirm or swap from spec alternatives before submitting. | R3.subtitle | ❌ |
| 3.4 | English promotional text (170 char max). Draft: *"Built because I kept reaching for my laptop after I was already in bed. BedTerm is a terminal you can actually use lying down, holding your phone in one hand."* (160) | R3.promotional_text | ❌ |
| 3.5 | English long description (4000 char max). Opens with the personal story; explains the keyboard bar / direction pad / no-chording differentiator; states the privacy posture; closes with a one-line founder note. | R3.description | ❌ |
| 3.6 | Keyword list (100 char, comma-separated): pick from {ssh, terminal, claude code, codex, vim, tmux, htop, remote, server, shell, mosh}. Front-load the highest-intent ones; never repeat words from the title or subtitle (Apple already indexes those). | R3.keywords | ❌ |
| 3.7 | What's New for v1.0: "First release. Thanks for trying BedTerm — feedback welcome." | R3.whats_new | ❌ |
| 3.8 | Translate fields 3.2–3.7 into zh-Hans, zh-Hant, ja, ko, ru. Maintain the in-bed framing — don't flatten to generic "mobile SSH". | R3.copy_localized | ❌ |
| 3.9 | Produce 1024×1024 marketing icon (no transparency, no rounded corners) and replace `BedTerm/Assets.xcassets/AppIcon.appiconset/` content if the current icon isn't final. | R4.app_icon_1024 | ❌ |
| 3.10 | Capture 6.9" iPhone screenshots from the iPhone 17 simulator. Minimum 3, target 5–6. Suggested set: (a) terminal showing the special-keys bar in context, (b) direction pad in use, (c) running Claude Code or vim, (d) connection setup screen, (e) landscape if supported, otherwise extra portrait. Save to `bedterm/docs/release/screenshots/iphone-6.9/`. | R4.iphone_6_9_screenshots | ❌ |
| 3.11 | Capture 13" iPad screenshots from an iPad simulator. Same content set, adapted. Save to `bedterm/docs/release/screenshots/ipad-13/`. | R4.ipad_screenshots | ❌ |
| 3.12 | Screenshot content is language-neutral (terminal output, generic shell prompts). One image set serves every locale. | R4.screenshots_localized | ❌ |
| 3.13 | Skip 6.5" iPhone screenshots unless ASC explicitly demands them at upload time. | R4.iphone_6_5_screenshots | ❌ |

## Phase 4 — Compliance, Build, Submit (PRD R6, R7, R8)

| # | Action | PRD rule | Status |
|---|---|---|---|
| 4.1 | Audit every `NS…UsageDescription` the app triggers. Today: `NSLocalNetworkUsageDescription` (already in `Info.plist`). Verify each has all 6 locales in `BedTerm/InfoPlist.xcstrings`. | R6.usage_descriptions | ❌ |
| 4.2 | Confirm `ITSAppUsesNonExemptEncryption=false` in `Info.plist` (already set). No export-compliance paperwork needed — SSH falls under the standard exempt-crypto exemption. | R6.encryption_export | ❌ |
| 4.3 | Complete the Age Rating questionnaire in ASC. Expected outcome: 4+. Answer "Unrestricted Web Access" carefully — SSH ≠ web browser; consult Apple's tooltip. | R6.age_rating | ❌ |
| 4.4 | Answer "Does your app contain, display, or access third-party content?" — declare OSS dependencies in their respective licenses (link in support page). | R6.content_rights | ❌ |
| 4.5 | Set `MARKETING_VERSION=1.0.0` and `CURRENT_PROJECT_VERSION=1` (or higher if you've already uploaded a build). | R7.version_build | ❌ |
| 4.6 | Verify Distribution certificate + App Store provisioning profile exist (Xcode Automatic signing handles this on first archive). | R7.distribution_cert | ❌ |
| 4.7 | Product → Archive (Release configuration) → Distribute App → App Store Connect → Upload. | R7.archive_uploads | ❌ |
| 4.8 | Wait for the build to finish processing in ASC. Add it to TestFlight internal testing. Install on your own device via TestFlight and smoke-test a real SSH connection end-to-end. | R7.testflight_internal | ❌ |
| 4.9 | Decide reviewer-access strategy. Default plan: spin up a $5/mo cloud VM with a throwaway non-privileged shell-only user, supply credentials in App Review notes. Document in `bedterm/docs/release/review-notes.md`. | R8.review_notes, R8.demo_account | ❌ |
| 4.10 | Submit the build for review. Use the same listing copy locked in Phase 3. | R8.submission_sent | ❌ |
| 4.11 | If rejected, read the feedback verbatim, fix, resubmit. If approved, mark `R8.review_passed` and ship. | R8.review_passed | ❌ |

## Risks and open questions (decide at phase boundary)

- **Trademark on "BedTerm".** Before action 3.2 (reserving the name in ASC), run a 5-minute search at [tmsearch.uspto.gov](https://tmsearch.uspto.gov/) in classes 9 (software) and 42 (SaaS). If a conflict appears, change the App Store name only (bundle ID stays — invisible to users) and update this spec's "Locked-in decisions" table.
- **Required-reason API list.** Determined during action 2.8 by greping the codebase. If we miss one, Apple sends a non-blocking warning post-upload that we can fix in a 1.0.1.
- **Demo SSH host risk.** A public SSH host with known credentials is a liability if left running. Bring it up only during the review window; tear it down once approved. Restrict the user to a chroot or container; firewall outbound.

## What this spec does NOT cover

- Adding analytics, crash reporting, or any data-collection SDK (would invalidate Data Not Collected).
- IAP, subscriptions, or any monetisation (v1 is free).
- macOS Catalyst or visionOS targets.
- Custom EULA.
- A sample-mode in-app demo for reviewers (fallback only if the demo-host approach is rejected by App Review).

## Working agreement

- One phase = one PR. Phase 1 lands first; later phases can run sequentially.
- After each completed action, flip the matching `<rule>` in `prd/bedterm/release.xml` to `status="✅"` and run `prd format prd/bedterm/release.xml`.
- Never mark a PRD bug `Fixed` without manual user confirmation (project rule).
