# Groups on real MLS — design

Date: 2026-06-16
Status: approved (pending spec review)

## Goal & scope

Add group chats with real end-to-end encryption via MLS:

- Create a group with N members.
- Add / remove members (owner only).
- Leave; when the owner leaves they pick a successor and ownership transfers.
- Group name + avatar, end-to-end (server stays blind).

The same client-side MLS-establishment flow fixes direct chats, which today
never build a client MLS group and fall back to plaintext message envelopes
(`send_text`: "MLS not ready, sending plaintext"). New direct chats will
establish a real MLS group. Existing legacy direct chats are NOT migrated in
this work (they keep working as-is); a later pass can re-establish them.

## Current state (findings)

- **Server**: full group support already exists — `POST /v1/groups`
  (`create_group`), `GET /v1/groups/{id}/members`, `POST /v1/groups/{id}/commit`
  (`post_commit`), welcomes endpoints, `claim_keypackage`. Table
  `mls_group_members` has `role_in_chat` (owner/member).
- **Core MLS** (`messenger-core/src/mls/group.rs`): `create_group(creator,
  group_id, [keypackages]) -> CreateGroupOutput { initial_commit, welcomes,
  group_state }`, `propose_add`, `propose_remove`, `process_commit`,
  `join_via_welcome`, `merge_pending`. OpenMLS emits ONE batched welcome that
  covers all added members.
- **Client**: never calls `claim_keypackage` + `create_group` for user chats.
  Direct chat = server group row (epoch 0, 2 members) with no MLS commit/welcome
  → plaintext fallback. The commit/welcome posting pattern DOES work in
  `settings/devices.rs` (multi-device provisioning via `propose_add` +
  `post_commit`). `chat/new_chat_dialog.rs` already has a "group" tab (multi-user
  select + name + `on_create_group` callback) but the callback is unwired.
- **Proto mismatch**: client `proto::mls::CreateGroupRequest` is out of sync with
  the server's. Client has `{ group_type, initial_commit, welcomes, ratchet_tree }`;
  server expects `{ group_type, initial_commit, welcomes: [WelcomeForDevice],
  member_devices: [MemberDeviceInit{user_id, device_id, role_in_chat}] }`. This
  is why `create_group` was never exercised from the client.

## Architecture

New client module `state/group_service.rs`, plus reuse of existing core MLS
primitives and server endpoints. UI in `chat/new_chat_dialog.rs` (wiring) and a
group management surface (extend `chat/profile_sheet.rs` or new
`chat/group_sheet.rs`).

### MLS-establishment primitive (shared by direct + group)

`establish_group(group_type, members: Vec<UserId>) -> Result<Uuid>`:

1. For each member user, enumerate their active devices; `claim_keypackage(user,
   device)` for each (multi-device aware). Empty pool → server returns a
   last-resort keypackage (already supported).
2. `core.create_group(creator, local_group_id, [all keypackages])` →
   `CreateGroupOutput { initial_commit, welcomes, group_state }`. The single
   batched welcome is replicated into one `WelcomePayload { recipient_device_id,
   welcome_ciphertext }` per recipient device.
3. `POST /v1/groups` with `group_type`, `initial_commit`, `welcomes[]`,
   `member_devices[]` (each `{user_id, device_id, role_in_chat}`; creator =
   owner). Server returns the authoritative `group_id`.
4. Members join through the existing path: `list_welcomes` → `join_via_welcome`
   → `ack` (already used in provisioning and `login_qr`).

`create_direct_chat` is reimplemented on top of this primitive (group_type
"direct", 2 members), replacing the current server-only/no-MLS path.

### Add / remove / leave / transfer

Commit flow mirrors `settings/devices.rs` (`propose_add` → `post_commit` with
welcomes + `member_changes`).

- **Add** (owner): `claim_keypackage` of the new members' devices →
  `core.propose_add` → `post_commit` (commit + welcomes + member_changes:add).
  New members join via welcome. Owner re-broadcasts current group name/avatar
  (see metadata) so newcomers converge.
- **Remove** (owner): `core.propose_remove(device)` → `post_commit`
  (member_changes:remove). Server applies member_changes to `mls_group_members`
  (verify `post_commit` already does this; if not, add it).
- **Leave** (member): self-remove → commit.
- **Leave (owner)**: UI requires picking a successor. Flow: transfer ownership
  to the chosen member, then owner self-removes via commit. If the owner is the
  sole member, leaving deletes the group.

### Ownership & roles

Roles are app-level metadata in `mls_group_members.role_in_chat`, not an MLS
operation. Ownership transfer = update the successor's role to `owner` (and the
old owner to `member` before they leave). New server endpoint `POST
/v1/groups/{id}/owner` (body: `new_owner_user_id`), authorized: only the current
owner. After a successful transfer the client broadcasts an E2E `SystemNote`
("ownership transferred") so all clients reflect the new owner in the member
list.

**Server-side authorization**: `create_group` already sets the caller as owner.
Add owner-only enforcement to the membership-mutating server paths
(`post_commit` when it carries add/remove member_changes, and the new owner
endpoint): reject with 403 if the caller is not the group's owner. This makes
owner-only real, not just UI gating.

### Group name + avatar (end-to-end)

`mls_groups` has no name/avatar columns (server is blind). Carry them E2E like
avatars, via a new application message:

- `AppMessageKind::GroupUpdate`
- `AppMessageBody::GroupUpdate { name: Option<String>, avatar_blob_id:
  Option<Uuid>, decryption_key: Vec<u8>, mime: String }`

The avatar picture lives in the encrypted attachment store (same mechanism as
`AvatarUpdate`); only the reference + key travel inside MLS. Owner sends a
`GroupUpdate` immediately after creation (name, and avatar if chosen), and again
on each add (cheap) so newcomers get the current values. Clients apply it to the
existing `display_name_cache` (keyed by group_id) and the avatar cache. The
mock/bridge `thumbnail`/avatar plumbing is reused.

### Proto / server alignment

Align client `proto::mls::CreateGroupRequest` to the server: add
`MemberDeviceInit { user_id, device_id, role_in_chat }`, change `welcomes` to
match `WelcomeForDevice` field-for-field (already `{recipient_device_id,
welcome_ciphertext}` — name parity), drop `ratchet_tree` (the tree travels in
the welcome via `use_ratchet_tree_extension(true)`). Verify the server tolerates
the absence of `ratchet_tree` (it already ignores it — not a field on the server
struct). Server changes are minimal: possibly apply member_changes:remove in
`post_commit`, the owner-transfer endpoint, and owner-only authz.

## UI

- `chat/new_chat_dialog.rs`: the "group" tab exists; wire `on_create_group(members,
  name)` → `group_service.establish_group("group", members)` then send the
  initial `GroupUpdate` (name + optional avatar picked in the dialog).
- Group management surface (extend `chat/profile_sheet.rs` or new
  `chat/group_sheet.rs`): member list (`get_group_members` + name resolution),
  add/remove buttons (visible only to owner), "leave" (owner → successor picker),
  edit name/avatar (owner). 
- Sidebar already renders `ChatType::Group` with a default icon; group avatar
  overrides it once received.
- Join/leave/ownership events shown as `SystemNote` bubbles.

## Error handling / edge cases

- **Epoch conflict** on commit (server 409): refetch members/epoch, rebuild the
  proposal, retry once.
- **Member with no active devices**: skip with a warning, like the direct
  `noDevices` case.
- **Partial welcome delivery**: a member that missed its welcome joins on the
  next sync (welcomes persist server-side until acked).
- **Empty keypackage pool**: server falls back to the last-resort keypackage.
- **Owner leaves as sole member**: deletes the group.

## Testing

- Core unit tests: `create_group(N)`, `propose_add`/`propose_remove` roundtrip
  (some already exist in `group.rs`).
- Integration (if an alice/bob/carol harness exists): create a 3-member group,
  send a message decryptable by all, add a 4th, remove one, transfer ownership,
  owner leaves.
- Manual: device + web run — create group, exchange messages, manage members,
  set name/avatar.

## Phasing (separate commits)

1. Proto alignment of `CreateGroupRequest` (+ `MemberDeviceInit`); server
   member_changes:remove in `post_commit` if missing; owner-only authz scaffolding.
2. `group_service.establish_group`; switch `create_direct_chat` onto it (fixes
   direct-chat E2E).
3. Group creation wired from `new_chat_dialog`; `GroupUpdate` name/avatar.
4. Member-list UI + add / remove / leave / ownership transfer (+ owner endpoint).
5. System notes, polish, deploy + signed APK.

## Non-goals (v1)

- Migrating existing legacy direct chats to real MLS.
- Per-member granular permissions beyond owner vs member.
- Public/joinable groups, invite links.
