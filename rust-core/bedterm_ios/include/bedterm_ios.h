// bedterm_ios FFI declarations.
// Appended to bedterm_core.h by build-rust-xcframework.sh after cbindgen
// regenerates the core header. Do not place include guards here — this
// fragment is spliced inside the existing BEDTERM_CORE_H guard.

// ── bedterm_ios ──────────────────────────────────────────────────────────

/// Callback invoked on the main thread when the iOS terminal's back button
/// is tapped.
typedef void (*BtIosBackCallback)(void *ctx);

// ── VC ──────────────────────────────────────────────────────────────────────

/// Create the iOS terminal `UIViewController *` (returned as opaque `void *`).
/// The returned pointer is +1 retained; release with `bt_ios_release_vc`.
/// The navigation title is set by the caller via SwiftUI `.navigationTitle()`.
///
/// @param on_back    Back-tap callback (may be NULL).
/// @param ctx        Context pointer passed through to `on_back` (may be NULL).
void *bt_ios_create_vc(BtIosBackCallback on_back,
                       void *ctx);

/// Release a `UIViewController *` previously returned by `bt_ios_create_vc`.
/// Safe to call with NULL.
void bt_ios_release_vc(void *vc_ptr);

// ── Settings VC ─────────────────────────────────────────────────────────────

/// Callback invoked on the main thread when the Rust Settings sheet's
/// navigation-bar Done button is tapped.
typedef void (*BtIosSettingsDoneCallback)(void *ctx);

/// Create the iOS Rust-built Settings `UIViewController *` (returned as
/// opaque `void *`). +1 retained; release with
/// `bt_ios_release_settings_vc`. The caller wraps the returned VC in a
/// `UINavigationController` and presents it as a `.formSheet`.
///
/// @param on_done  Done-tap callback (may be NULL).
/// @param ctx      Context pointer passed through to `on_done` (may be NULL).
void *bt_ios_create_settings_vc(BtIosSettingsDoneCallback on_done,
                                void *ctx);

/// Release a Settings `UIViewController *` previously returned by
/// `bt_ios_create_settings_vc`. Safe to call with NULL.
void bt_ios_release_settings_vc(void *vc_ptr);

// ── View ────────────────────────────────────────────────────────────────────

/// Resolve the `BtIosMetalInputView *` embedded inside a VC returned by
/// `bt_ios_create_vc`. Returns NULL when the view didn't construct.
void *bt_ios_vc_metal_view(void *vc_ptr);

/// Feed raw terminal bytes into a `BtIosMetalInputView`'s owned grid.
/// `view_ptr` must be a live `BtIosMetalInputView *`. No-op on NULL / empty.
void bt_ios_view_feed_bytes(void *view_ptr,
                            const unsigned char *bytes,
                            uintptr_t len);

/// Query the cell pixel size (width, height) as reported by the view's
/// glyph atlas. Either out-param may be NULL. Returns zeros if the view
/// has no live renderer.
void bt_ios_view_cell_size_px(void *view_ptr,
                              uint32_t *out_w,
                              uint32_t *out_h);

/// C-style PTY-byte sink callback invoked when the metal view emits
/// hardware-keyboard / IME-commit bytes. `bytes` is valid only for the
/// duration of the call.
typedef void (*BtIosOnSendCallback)(void *ctx,
                                    const unsigned char *bytes,
                                    uintptr_t len);

/// Install a C-callback PTY sink on the metal view. Pass `cb = NULL` to
/// clear. Replaces any prior sink.
void bt_ios_view_set_on_send(void *view_ptr,
                             BtIosOnSendCallback cb,
                             void *ctx);

/// C-style resize callback: fired from the metal view's `layoutSubviews`
/// whenever the renderer-derived `(cols, rows)` changes. Main-thread only.
typedef void (*BtIosOnResizeCallback)(void *ctx,
                                      uint16_t cols,
                                      uint16_t rows);

/// Install a resize-notification callback on the metal view. The callback
/// fires once per change of the renderer-derived `(cols, rows)`. Pass
/// `cb = NULL` to clear.
void bt_ios_view_set_on_resize(void *view_ptr,
                               BtIosOnResizeCallback cb,
                               void *ctx);

/// Read the most recent `(cols, rows)` the metal view derived from its
/// bounds + cell pixel size. Either out-param may be NULL. Returns
/// `(0, 0)` before the first layout pass.
void bt_ios_view_grid_dim(void *view_ptr,
                          uint16_t *out_cols,
                          uint16_t *out_rows);

// ── Onboarding VCs (R10 Rust port) ──────────────────────────────────────────

/// Callback fired when the user picks a choice on a picker-step VC.
/// `choice` is the discriminant of the picked option:
///   HostKindVC:  0 = macOS, 1 = Linux/other
///   LocationVC:  0 = same-Wi-Fi, 1 = remote
typedef void (*BtIosOnboardingChoiceCallback)(void *ctx, int32_t choice);

/// Callback fired when the user taps the primary "Continue" button on the
/// MacTutorial or LocalPermission step.
typedef void (*BtIosOnboardingContinueCallback)(void *ctx);

/// Create the host-kind onboarding step VC (returned as opaque `void *`).
/// The returned pointer is +1 retained; release with `bt_ios_release_vc`.
void *bt_ios_create_onboarding_host_kind_vc(BtIosOnboardingChoiceCallback on_choice,
                                            void *ctx);

/// Create the location onboarding step VC. Same release contract.
void *bt_ios_create_onboarding_location_vc(BtIosOnboardingChoiceCallback on_choice,
                                           void *ctx);

/// Create the macOS-tutorial onboarding step VC. Fires `on_continue` once
/// on Continue tap. Same release contract.
void *bt_ios_create_onboarding_mac_tutorial_vc(BtIosOnboardingContinueCallback on_continue,
                                               void *ctx);

/// Create the local-network-permission terminator step VC. `is_remote`
/// selects the "All set" copy variant; pass `false` for the same-Wi-Fi
/// prose. The actual Bonjour probe lives in Swift — this VC just fires
/// `on_continue(ctx)` on tap.
void *bt_ios_create_onboarding_local_permission_vc(BtIosOnboardingContinueCallback on_continue,
                                                   void *ctx,
                                                   bool is_remote);

/// Callback fired once when the entire onboarding flow has completed.
/// The Swift host swaps to the hosts root in response. The `ctx`
/// pointer is the same value the host passed into
/// `bt_ios_create_onboarding_flow_vc`.
typedef void (*BtIosOnboardingFlowCompletedCallback)(void *ctx);

/// Create the Rust-owned onboarding flow VC — a
/// `UINavigationController` subclass that owns the R10 state machine and
/// pushes each step VC as the user advances. The Swift host installs the
/// returned VC as the window root and reacts to `on_completed(ctx)` by
/// swapping to the hosts root.
///
/// Returns a `+1` retained `UIViewController *`; release with
/// `bt_ios_release_vc`. The coordinator reaches back into Swift via
/// `bt_swift_request_local_network` (Bonjour probe) and
/// `bt_swift_onboarding_set_completed` (UserDefaults flag).
void *bt_ios_create_onboarding_flow_vc(BtIosOnboardingFlowCompletedCallback on_completed,
                                       void *ctx);

// ── Hosts list VC (W24b R-port phase 1) ─────────────────────────────────────

/// Callback fired on the main thread when the user taps the navigation-bar
/// `+` button on the Rust hosts list VC. Swift owns the response
/// (presenting the SwiftUI connect-form sheet).
typedef void (*BtIosHostsAddCallback)(void *ctx);

/// Create the iOS Rust-built Hosts list `UIViewController *` (returned as
/// opaque `void *`). +1 retained; release with
/// `bt_ios_release_hosts_list_vc`.
///
/// Row taps invoke `bt_swift_hosts_connect(id)` directly; swipe-delete
/// invokes `bt_swift_hosts_delete(id)`. Both round-trip through the
/// `@_cdecl` shims in `HostsBridge.swift`.
///
/// @param on_add  `+`-tap callback (may be NULL).
/// @param ctx     Context pointer passed through to `on_add` (may be NULL).
void *bt_ios_create_hosts_list_vc(BtIosHostsAddCallback on_add,
                                  void *ctx);

/// Release a Hosts list `UIViewController *` previously returned by
/// `bt_ios_create_hosts_list_vc`. Safe to call with NULL.
void bt_ios_release_hosts_list_vc(void *vc_ptr);

// ── Connect form VC (W24c R-port) ───────────────────────────────────────────

/// Callback fired on the main thread when the user successfully saves the
/// connect-form. `id_string` is a UTF-8 nul-terminated UUID of the saved
/// entry, valid only for the duration of the call. `connect_now` mirrors
/// the SwiftUI `connectOnSave` shortcut.
typedef void (*BtIosConnectFormDoneCallback)(void *ctx,
                                             const char *id_string,
                                             bool connect_now);

/// Callback fired on the main thread when the user taps Cancel on the
/// Rust connect-form VC.
typedef void (*BtIosConnectFormCancelCallback)(void *ctx);

/// Create the iOS Rust-built connect-form `UIViewController *` (returned
/// as opaque `void *`). +1 retained; release via
/// `bt_ios_release_connect_form_vc`.
///
/// @param editing_id_or_null  UTF-8, nul-terminated UUID-string of an
///                            existing host to edit, or NULL for "Add Host".
/// @param on_done             Save-success callback (may be NULL).
/// @param on_cancel           Cancel-tap callback (may be NULL).
/// @param ctx                 Context pointer threaded into both callbacks.
void *bt_ios_create_connect_form_vc(const char *editing_id_or_null,
                                    BtIosConnectFormDoneCallback on_done,
                                    BtIosConnectFormCancelCallback on_cancel,
                                    void *ctx);

/// Release a connect-form `UIViewController *` previously returned by
/// `bt_ios_create_connect_form_vc`. Safe to call with NULL.
void bt_ios_release_connect_form_vc(void *vc_ptr);

// ── SSH bridge ──────────────────────────────────────────────────────────────
//
// The Rust-side TerminalSession port (not yet landed) will drive Swift's
// CitadelSSHClient through a function-pointer vtable filled by Swift.
// Declarations below pin down the C ABI early so the Swift wrapper and the
// Rust trait stay in lock-step.

/// Result code passed to every completion callback. Mirrors `SSHError`.
typedef enum BtSSHResultCode {
  BtSSHResultOk = 0,
  BtSSHResultDnsResolution = 1,
  BtSSHResultTcpRefused = 2,
  BtSSHResultTimeout = 3,
  BtSSHResultHandshakeFailed = 4,
  BtSSHResultAuthenticationFailed = 5,
  BtSSHResultPrivateKeyParse = 6,
  BtSSHResultPrivateKeyPassphraseRequired = 7,
  BtSSHResultHostKeyMismatch = 8,
  BtSSHResultDisconnected = 9,
  BtSSHResultPeerReset = 10,
  BtSSHResultShellExited = 11,
  BtSSHResultOther = 99,
} BtSSHResultCode;

/// C mirror of `SSHConnectionRequest`. Lifetimes: every pointer is borrowed
/// only for the duration of the `connect` call.
typedef struct BtSSHConnectRequest {
  const void *credential_opaque;
  int cols;
  int rows;
  /// UTF-8, nul-terminated; NULL == no bootstrap payload.
  const char *bootstrap_payload;
} BtSSHConnectRequest;

/// Completion callback for connect/write/resize/disconnect. `msg` is a
/// retained `NSString *` (may be NULL); the receiver releases it. `extra`
/// carries shell-exit codes etc.
typedef void (*BtSSHCompletion)(void *ctx,
                                BtSSHResultCode code,
                                const void *msg,
                                int extra);

/// Output-data sink installed by Rust on the bridge. Called per inbound
/// chunk on the main queue. `bytes` is valid only for the duration of
/// the call.
typedef void (*BtSSHOutputSink)(void *ctx, const unsigned char *bytes, uintptr_t len);

/// Function-pointer table filled by Swift's `SSHClientBridge`.
typedef struct BtSSHClientVTable {
  void (*connect)(void *ctx,
                  BtSSHConnectRequest req,
                  BtSSHCompletion completion,
                  void *completion_ctx);
  void (*write)(void *ctx,
                const unsigned char *bytes,
                uintptr_t len,
                BtSSHCompletion completion,
                void *completion_ctx);
  void (*resize)(void *ctx,
                 int cols,
                 int rows,
                 BtSSHCompletion completion,
                 void *completion_ctx);
  void (*disconnect)(void *ctx,
                     BtSSHCompletion completion,
                     void *completion_ctx);
  void (*set_output_sink)(void *ctx, BtSSHOutputSink sink, void *sink_ctx);
  /// Balance the +1 retain Swift gave us when constructing `ctx`.
  void (*release)(void *ctx);
} BtSSHClientVTable;

/// Opaque Rust-owned handle wrapping a Swift `SSHClientBridge` vtable + ctx.
/// Returned by `bt_ios_register_ssh_bridge`; released with
/// `bt_ios_ssh_bridge_release`.
typedef struct SSHBridgeHandle SSHBridgeHandle;

/// Register a Swift-built SSH bridge with the Rust side. `vtable` is copied
/// by value; `ctx` ownership transfers to the returned handle (its `Drop`
/// invokes `vtable.release(ctx)`).
SSHBridgeHandle *bt_ios_register_ssh_bridge(const BtSSHClientVTable *vtable,
                                            void *ctx);

/// Release a handle previously returned by `bt_ios_register_ssh_bridge`.
/// Safe to call with NULL.
void bt_ios_ssh_bridge_release(SSHBridgeHandle *handle);
