# Legal assessment: a `steamclient` replacement (M4)

Written 2026-11-20 by the project's AI assistant at the maintainer's
request, because a paid review is not affordable. **This is not legal
advice.** It is a careful reading of the law and the contracts, meant to
set safe working rules. If the project grows, or Valve objects, a lawyer
should check it. Free first advice in Germany is offered by student law
clinics at many universities (permitted by § 6 RDG).

Scope: German and EU law, because the maintainer and most early users
live in Germany. US law (GitHub, DMCA) is covered only as a practical risk.

## 1. Sources read

- UrhG: § 69a(2) (ideas and principles behind interfaces are not
  protected), § 69d(3) (the rightful user may observe, study and test a
  program while running it), § 69e (decompilation for interoperability,
  under strict conditions), and § 69g(2) (contract terms contrary to
  § 69d(3) or § 69e are void). Text from gesetze-im-internet.de.
- CJEU C-406/10, *SAS Institute v World Programming* (2 May 2012): the
  functionality of a program, its programming language and its data file
  formats are not protected by copyright. A licensee may observe, study
  and test the program to find its underlying ideas and principles, and a
  licence cannot prevent this. Sources: the CURIA press release 53/12 and
  BAILII.
- Steamworks SDK License (from ValveSoftware/Proton `lsteamclient/LICENSE`).
- Steam Subscriber Agreement (SSA): § 2.G, § 4.B and § 10, read on
  store.steampowered.com.

## 2. Copyright

**What may not be copied:** the SDK's header files, documentation text and
code. They are Valve's expression, so they are never copied, translated
or used as the basis for generated code (red line 6).

**What is free to reimplement:** the ideas and principles behind
interfaces (§ 69a(2)) and the functionality (SAS). That covers what a call
does, which interface versions exist, the order of methods in a table,
the size and meaning of arguments, and the result codes. These are
technical facts that every compatible program has to match. A
reimplementation written from scratch in Rust, with its own structure and
names chosen for Reborn, is therefore very likely not an infringement.
Whether the exact layout of a large interface could count as protected
expression has not been decided by a court. Because the layout is
dictated by the binary interface, that is unlikely.

**How the knowledge may be obtained:**

- **Observation (§ 69d(3)):** the user owns their games, including the
  `steam_api` library inside them, and may run them with the official
  client. While doing that, they may observe what the game asks of
  `steamclient`: the version strings, the slot number of each call, the
  arguments and the results. This right cannot be removed by contract
  (§ 69g(2); SAS). This is the main source for the shim.
- **Public documentation:** Valve's public API reference on its website
  describes what each function does. Learning from it is fine. Copying its
  text is not.
- **Decompilation (§ 69e):** only allowed when indispensable, when the
  information is not readily available otherwise, and limited to what
  interoperability needs. Reborn avoids it. If it ever becomes necessary,
  it needs its own ADR.

## 3. Contracts

**Steamworks SDK License:** § 2.4 forbids licensees to reverse engineer the
SDK or to develop software that replaces it. It binds only people who
downloaded or used the SDK (§ 3.1). For them, § 69g(2) probably voids the
ban on observing and studying, but probably not the separate promise not
to build a replacement, and that contract is governed by Washington law
(§ 6.4). The consequences:

- Nobody who has accepted the SDK License works on the shim. This cannot
  be checked. A false statement is a breach of that person's own contract,
  and the code stays clean only through its provenance records (ADR 0012).
  A tainted contribution is removed and rebuilt from the recordings.
- Nobody downloads the SDK, reads files distributed under it (Proton
  `lsteamclient/`, `steamworks-sys`, the gbe_fork `sdk/` folder), or adds
  a dependency that bundles it.

**Steam Subscriber Agreement (every user):**

- § 2.G forbids reverse engineering and "emulate or redirect the
  communication protocols used by Valve" without consent, but "subject to
  exceptions under applicable law".
- § 10 makes the law of the user's country of residence apply for EU
  users. For German users, the parts that conflict with § 69d(3) and § 69e
  are therefore void (§ 69g(2)).
- The protocol clause goes beyond copyright. ADR 0001 already accepted
  that risk for all of Reborn: it is a risk to the user's account, not a
  criminal or copyright risk.
- The shim adds one thing: the game talks to Reborn instead of the
  official client. Valve could treat that as a breach and act against
  accounts.
- Mitigation: the shim is opt-in per game, with a clear warning, and
  developers test with secondary accounts.

## 4. Technical protection measures

- § 95a UrhG protects "effective technological measures", and § 69f(2)
  covers tools made only to remove program protection.
- The shim removes nothing, patches nothing, and passes on only what
  Valve's servers issue: real tickets for real licenses. A wrapped game
  that starts because it gets genuine answers is used as licensed, not
  circumvented.
- This argument fails as soon as the shim treats a protection measure
  specially. Hence the rule, in line with red line 1: Reborn never detects,
  answers, imitates or works around a DRM check. Games that do not start
  stay unsupported.

## 5. Trademarks

- Games load a file named `steamclient(64).dll` or `steamclient.so`.
  Using that file name is a technical necessity. § 23 MarkenG allows a
  mark to be used where it is needed to indicate the purpose of a product
  (for example spare parts), as long as this follows honest practices.
- The product stays "Reborn". There is no Valve logo, the shim is not
  called "Steam …" anywhere in Reborn's own UI, and Reborn states that it
  is not affiliated with Valve (red line 6).

## 6. Practical risks (not about who is right)

- **Takedown:** GitHub follows US DMCA notices. Valve could send one for
  the shim, rightly or not. A counter-notice is possible, but the code may
  be offline for a while. Mitigation: the shim lives in its own crate with
  its own provenance records, so it could move to a separate repository
  without affecting the client.
- **Cease-and-desist letter (Abmahnung):** in Germany this can be
  expensive. Clean provenance records are the best defence: every
  interface detail with its observation source, like the rule "no
  protocol behaviour without a source".
- **Account bans:** see § 3. Reborn makes this risk visible to users.

## 7. Result

A shim is defensible under German and EU law if all of these hold:

1. Clean room: no SDK, no SDK-licensed files, and no contributor who has
   accepted the SDK License.
2. Interface details come only from observing games the user owns while
   they run with the official client (§ 69d(3)), plus Valve's public
   documentation for meaning. Each detail is recorded with its source.
3. Ownership, tickets and achievements come only from Valve (red lines 3
   and 4). DRM is never treated specially (red line 1).
4. Opt-in per game, with a warning about the account risk.
5. Reborn's own name and identity; the `steamclient` file name only
   because games require it.
