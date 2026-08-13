//! Tests for the DNS-rebinding defence: what `Host` values the policy
//! accepts, which `allowed_hosts` entries refuse to start, and the local
//! names a reachable bind answers to. Split out of `host.rs` to keep it
//! under the size budget.

use super::*;

/// [`HostPolicy::from_bind_with_local_names`] for a case whose entries are
/// well-formed. The rejections live in their own tests below, where the
/// error is the assertion rather than a `.expect` nobody reads.
fn policy_for_bind(
    bind_host: &str,
    port: u16,
    extra_hosts: &[String],
    local_names: &[String],
) -> HostPolicy {
    HostPolicy::from_bind_with_local_names(bind_host, port, extra_hosts, local_names)
        .expect("well-formed allowed_hosts")
}

#[test]
fn a_wildcard_bind_accepts_lan_ips_but_never_an_unrelated_name() {
    // `--host 0.0.0.0` is the LAN case: the machine's own addresses cannot
    // be enumerated, and an IP-literal Host cannot come from rebinding
    // (that needs a name). Every name other than this machine's own still
    // has to be allow-listed — supplied empty here so the assertions pin
    // the IP-literal rule alone, whatever the box running them is called.
    let policy = policy_for_bind("0.0.0.0", 3000, &[], &[]);
    assert!(policy.accepts_authority("192.168.0.16:3000"));
    assert!(policy.accepts_authority("[fd00::1]:3000"));
    assert!(!policy.accepts_authority("192.168.0.16:3001"));
    assert!(!policy.accepts_authority("evil.test:3000"));
    assert!(!policy.accepts_authority("nas.local:3000"));

    // A specific bind does not get the IP-literal allowance.
    let loopback = policy_for_bind("127.0.0.1", 3000, &[], &[]);
    assert!(!loopback.accepts_authority("192.168.0.16:3000"));
}

#[test]
fn configured_public_names_are_accepted_bare_and_on_the_bind_port() {
    // `cru tunnel` / reverse proxy: the browser's Host is the public name,
    // with no port when the proxy terminates on 80 or 443.
    let policy = policy_for_bind(
        "127.0.0.1",
        3000,
        &[
            "crucible.example.com".to_string(),
            "old.example:8443".into(),
        ],
        &[],
    );
    assert!(policy.accepts_authority("crucible.example.com"));
    assert!(policy.accepts_authority("crucible.example.com:3000"));
    assert!(policy.accepts_authority("old.example:8443"));
    // A configured entry that names its own port is exact.
    assert!(!policy.accepts_authority("old.example"));
    assert!(!policy.accepts_authority("old.example:3000"));
    // Neighbours of a configured name are not the configured name.
    assert!(!policy.accepts_authority("evil.crucible.example.com"));
    assert!(!policy.accepts_authority("crucible.example.com.evil.test"));
}

#[test]
fn a_remotely_reachable_bind_answers_to_this_machines_own_names() {
    // The LAN case the IP-literal rule does not cover: a browser on another
    // machine addresses this box by NAME, and that Host used to be refused
    // no matter how the operator bound the socket.
    let names = ["node7".to_string(), "node7.local".to_string()];
    let policy = policy_for_bind("0.0.0.0", 3000, &[], &names);

    assert!(policy.accepts_authority("node7:3000"));
    assert!(policy.accepts_authority("node7.local:3000"));
    // Same as a configured entry: bare too, for a proxy terminating on 80/443.
    assert!(policy.accepts_authority("node7"));
    // Everything the rule did NOT open stays shut.
    assert!(!policy.accepts_authority("node7:3001"));
    assert!(!policy.accepts_authority("evil.test:3000"));
    assert!(!policy.accepts_authority("node7.evil.test:3000"));
    assert!(!policy.accepts_authority("evil.node7:3000"));
}

#[test]
fn a_bind_to_one_lan_interface_also_answers_to_this_machines_names() {
    // Narrowing the bind to one interface does not change how a browser
    // spells the machine, so the names have to come along.
    let names = ["node7".to_string()];
    let policy = policy_for_bind("192.168.0.16", 3000, &[], &names);

    assert!(policy.accepts_authority("node7:3000"));
    assert!(policy.accepts_authority("192.168.0.16:3000"));
    // Still not a wildcard bind: other IP literals are not ours.
    assert!(!policy.accepts_authority("10.0.0.5:3000"));
}

#[test]
fn a_loopback_bind_answers_to_no_name_but_its_own() {
    // Nothing off this machine can reach a loopback socket, so there is no
    // LAN case to serve — and every name admitted here would be a
    // rebinding target bought for nothing.
    let names = ["node7".to_string(), "node7.local".to_string()];
    for bind in ["127.0.0.1", "::1", "localhost"] {
        let policy = policy_for_bind(bind, 3000, &[], &names);
        assert!(
            !policy.accepts_authority("node7:3000"),
            "{bind} bind must not admit the machine's LAN name"
        );
        assert!(policy.accepts_authority("localhost:3000"), "{bind}");
    }
}

#[test]
fn the_local_names_are_the_hostname_and_its_mdns_spelling() {
    // Derivation only — what the OS reports is environment, not contract.
    assert_eq!(local_names_from("node7", None), ["node7", "node7.local"]);
    // Already qualified: `.local` is not appended to a dotted name.
    assert_eq!(local_names_from("node7.lan", None), ["node7.lan"]);
    assert_eq!(local_names_from("Node7", None), ["node7", "node7.local"]);
    // A hostname that is not a usable authority yields nothing rather than
    // poisoning the allow-list with something unparseable.
    assert!(local_names_from("", None).is_empty());
    assert!(local_names_from("localhost", None).is_empty());
    assert!(local_names_from("not a hostname", None).is_empty());
}

#[test]
fn the_resolvers_canonical_name_extends_the_hostname_or_is_ignored() {
    // The case this exists for: a bare `/etc/hostname` whose FQDN lives in
    // DNS or `hosts`. Without it, a LAN browser at the machine's real
    // domain 403s while the same box answers to the bare name and an IP.
    assert_eq!(
        local_names_from("node7", Some("node7.example.com")),
        ["node7", "node7.local", "node7.example.com"]
    );
    // Spelling differences in the resolver's answer are not new names.
    assert_eq!(
        local_names_from("node7", Some("NODE7.example.com.")),
        ["node7", "node7.local", "node7.example.com"]
    );
    // Canonical name == hostname (the common answer): nothing to add.
    assert_eq!(
        local_names_from("node7", Some("node7")),
        ["node7", "node7.local"]
    );
    // An FQDN hostname whose resolver agrees stays a single name.
    assert_eq!(
        local_names_from("node7.example.com", Some("node7.example.com")),
        ["node7.example.com"]
    );

    // A canonical name that does NOT extend the hostname is not this
    // machine describing itself — admitting it would hand whoever controls
    // resolution a rebinding target, which is what the allow-list is for.
    for hostile in [
        "evil.test",
        "node7-evil.test",
        "notnode7.example.com",
        "evil.node7",
        "node7.",
        "",
    ] {
        assert_eq!(
            local_names_from("node7", Some(hostile)),
            ["node7", "node7.local"],
            "{hostile:?} must not reach the allow-list"
        );
    }
}

#[test]
fn a_reachable_bind_answers_to_the_resolved_fqdn() {
    // End to end through the policy: the FQDN is accepted bare and on the
    // bind port, exactly like the hostname it extends.
    let names = local_names_from("node7", Some("node7.example.com"));
    let policy = policy_for_bind("0.0.0.0", 3000, &[], &names);

    assert!(policy.accepts_authority("node7.example.com:3000"));
    assert!(policy.accepts_authority("node7.example.com"));
    assert!(policy.accepts_authority("node7:3000"));
    // The FQDN's neighbours are still not the FQDN.
    assert!(!policy.accepts_authority("evil.example.com:3000"));
    assert!(!policy.accepts_authority("node7.example.com.evil.test:3000"));
    assert!(!policy.accepts_authority("node7.example.com:3001"));

    // …and a loopback bind still answers to no LAN name, FQDN included.
    let loopback = policy_for_bind("127.0.0.1", 3000, &[], &names);
    assert!(!loopback.accepts_authority("node7.example.com:3000"));
}

#[test]
fn from_web_config_forwards_the_configured_allowed_hosts() {
    // The seam where a documented control silently becomes dead: drop
    // `allowed_hosts` here and every proxy deployment 403s while the config
    // key appears to work.
    let config = WebConfig {
        host: "0.0.0.0".to_string(),
        port: 3000,
        allowed_hosts: vec!["crucible.example.com".to_string()],
        ..WebConfig::default()
    };
    let policy = HostPolicy::from_web_config(&config).expect("well-formed config");
    assert!(policy.accepts_authority("crucible.example.com"));
    assert!(policy.accepts_authority("192.168.0.16:3000"));
    assert!(!policy.accepts_authority("evil.test:3000"));
}

#[test]
fn normalize_authority_rejects_everything_that_is_not_a_bare_authority() {
    for raw in [
        "",
        " ",
        "host:",
        "host:0",
        "host:99999",
        "::1:3000",         // unbracketed IPv6
        "[::1:3000",        // unterminated bracket
        "[not-an-ip]:3000", // brackets must hold an IPv6 literal
        "host/path",
        "host?q=1",
        "http://host:3000",
        "ho st:3000",
        "host\u{00e9}:3000",
        "%31%32%37.0.0.1:3000",
        "..:3000",
        ".host:3000",
    ] {
        assert_eq!(normalize_authority(raw), None, "{raw:?} must not parse");
    }

    // …and canonicalizes the spellings that ARE legal.
    assert_eq!(
        normalize_authority("LocalHost.:3000").as_deref(),
        Some("localhost:3000")
    );
    assert_eq!(
        normalize_authority("[0:0:0:0:0:0:0:1]:03000").as_deref(),
        Some("[::1]:3000")
    );
}

#[test]
fn the_host_shapes_upstream_host_checks_get_wrong_are_parsed_or_refused() {
    // Ported from the regression suites of Django, Rails and Werkzeug —
    // every one of these is a shape that got some framework's Host check
    // wrong at least once.
    for raw in [
        "example.com:80",
        "[2001:19f0:feee::dead:beef:cafe]:8080",
        // Punycode is ASCII by construction, so an IDN needs no special case.
        "xn--4ca9at.com",
        // The fully-qualified spelling of the same name.
        "example.com.",
    ] {
        assert!(normalize_authority(raw).is_some(), "{raw:?} must parse");
    }

    for raw in [
        "example.com..",
        "example.com@evil.tld",
        "example.com:80/badpath",
        "www.example.com:80:80",
        // Django accepts a leading dot in the *Host* header, which turns
        // its own suffix syntax into a bypass. This one does not.
        ".example.com",
        "..example.com",
    ] {
        assert_eq!(normalize_authority(raw), None, "{raw:?} must not parse");
    }
}

#[test]
fn a_suffix_entry_admits_the_apex_and_exactly_one_label_beneath_it() {
    // `.example.com` is the leading-dot syntax Django, Rails, Vite,
    // webpack-dev-server and Werkzeug all share — with Rails' narrower
    // depth rule, because any-depth matching inherits every delegated
    // subtree under the apex.
    let policy = policy_for_bind("127.0.0.1", 3000, &[".example.com".to_string()], &[]);

    assert!(policy.accepts_authority("example.com"));
    assert!(policy.accepts_authority("app.example.com"));
    assert!(policy.accepts_authority("app.example.com:3000"));
    // Two labels deep is where a dangling NS record under the apex would
    // hand an attacker a name inside it.
    assert!(!policy.accepts_authority("a.b.example.com"));
    assert!(!policy.accepts_authority("a.b.example.com:3000"));
}

#[test]
fn a_suffix_entry_refuses_a_name_that_merely_ends_with_its_apex() {
    let policy = policy_for_bind("127.0.0.1", 3000, &[".example.com".to_string()], &[]);

    // `ends_with("example.com")` — the classic form of this bug.
    assert!(!policy.accepts_authority("evilexample.com"));
    // Rails' CVE-2021-22903: the apex interpolated into a regex unescaped,
    // where `.` matched the `-`.
    assert!(!policy.accepts_authority("sub-example.com"));
    // The apex as a prefix of somebody else's name.
    assert!(!policy.accepts_authority("example.com.evil.test"));
    // A Host that is only the suffix — refused at the parse step, and again
    // by the empty-label rule if it ever got past it.
    assert!(!policy.answers_to(".example.com"));
    assert!(!policy.answers_to("..example.com"));
    // A different apex of the same shape.
    assert!(!policy.accepts_authority("app.example.test"));
}

#[test]
fn an_ip_literal_host_is_never_matched_by_a_suffix_entry() {
    // The entry is contrived on purpose: a rule that admits *names* under a
    // domain must have no way to reach an ADDRESS, whatever the apex looks
    // like. Without that rule `.0.0.1` would hand out every 10.0.0.x host,
    // and the IP-literal path is exactly the one that is safe only because
    // there is no name involved.
    let policy = policy_for_bind("127.0.0.1", 3000, &[".0.0.1".to_string()], &[]);

    assert!(!policy.accepts_authority("10.0.0.1:3000"));
    assert!(!policy.accepts_authority("10.0.0.1"));
    assert!(!policy.accepts_authority("[::1]:3001"));
    // The loopback bind's own authority is unaffected.
    assert!(policy.accepts_authority("127.0.0.1:3000"));
}

#[test]
fn a_suffix_entry_follows_the_same_port_rule_as_an_exact_entry() {
    let policy = policy_for_bind(
        "127.0.0.1",
        3000,
        &[".example.com".to_string(), ".old.example:8443".to_string()],
        &[],
    );

    // No port on the entry: bare (a proxy terminating on 80/443) or the
    // port we bound.
    assert!(policy.accepts_authority("app.example.com"));
    assert!(policy.accepts_authority("app.example.com:3000"));
    assert!(!policy.accepts_authority("app.example.com:3001"));
    // A port on the entry is exact.
    assert!(policy.accepts_authority("app.old.example:8443"));
    assert!(policy.accepts_authority("old.example:8443"));
    assert!(!policy.accepts_authority("app.old.example"));
    assert!(!policy.accepts_authority("app.old.example:3000"));
}

#[test]
fn a_malformed_allowed_hosts_entry_refuses_to_start() {
    // The regression this exists for: `*.example.test` was dropped with one
    // `tracing::warn!` line, so the operator got a 403 from an allow-list
    // that looked configured and was empty.
    let refusals = [
        ("*.example.test", "a glob is not the syntax"),
        ("*", "nor is a bare star"),
        (".", "a bare dot names nothing"),
        ("..", "and neither does two"),
        (".:3000", "a port is not a name"),
        (
            "..example.test",
            "a doubled dot is not a name a browser sends",
        ),
        (".192.168.0.1", "an address cannot be a name suffix"),
        ("http://example.test", "a URL is not an authority"),
        ("example.test/path", "nor is a path"),
    ];
    for (entry, why) in refusals {
        let err =
            HostPolicy::from_bind_with_local_names("127.0.0.1", 3000, &[entry.to_string()], &[])
                .expect_err(why);
        // The message has to name the entry, or the operator cannot find it
        // in a config file with a dozen of them. Asserted on the QUOTED form
        // the error actually renders (`{entry:?}`): a bare `contains(entry)`
        // passes for free on the entries where finding it matters most —
        // `.` and `..` appear in any sentence with a period, and `*` appears
        // in the Unparseable message's own `*.example.com` hint.
        let quoted = format!("{entry:?}");
        assert!(err.to_string().contains(&quoted), "{err}");
    }
}

#[test]
fn a_public_suffix_allowed_hosts_entry_refuses_to_start() {
    // Vite's documentation puts it plainly: "you should never add Top-Level
    // Domains like .com to the list." Every name anyone can register under
    // one of these would be an authority this server answers to.
    for entry in [
        ".com",
        ".io",
        ".test",
        ".local",
        ".internal",
        ".co.uk",
        ".github.io",
        ".trycloudflare.com",
    ] {
        let err =
            HostPolicy::from_bind_with_local_names("127.0.0.1", 3000, &[entry.to_string()], &[])
                .expect_err(entry);
        assert_eq!(
            err,
            InvalidAllowedHost::PublicSuffix {
                entry: entry.to_string()
            },
            "{entry} must be refused as a public suffix"
        );
    }

    // …while a domain someone actually controls under one of them is fine.
    let policy = policy_for_bind("127.0.0.1", 3000, &[".crucible.co.uk".to_string()], &[]);
    assert!(policy.accepts_authority("app.crucible.co.uk"));
}

#[test]
fn a_suffix_entry_inside_an_undelegated_namespace_refuses_to_start() {
    // A registered domain gets you a subtree; mDNS and `.internal` do not.
    // Whoever answers the query owns the name while they answer it, so
    // `.node7.local` reads as "under my machine" and means "any name any LAN
    // peer claims" — which is the rebinding vehicle the allow-list exists to
    // refuse. Depth changes nothing about who may answer.
    for entry in [
        ".node7.local",
        ".deep.node7.local",
        ".svc.internal",
        ".box.home.arpa",
    ] {
        let err =
            HostPolicy::from_bind_with_local_names("127.0.0.1", 3000, &[entry.to_string()], &[])
                .expect_err(entry);
        assert_eq!(
            err,
            InvalidAllowedHost::PublicSuffix {
                entry: entry.to_string()
            },
            "{entry} must be refused however deep it sits"
        );
    }

    // The EXACT form stays available, and is the one to use: it names a
    // single authority instead of a namespace, and the real host already
    // holds that name against mDNS conflict resolution.
    let policy = policy_for_bind("0.0.0.0", 3000, &["node7.local".to_string()], &[]);
    assert!(policy.accepts_authority("node7.local:3000"));
    assert!(!policy.accepts_authority("evil.node7.local:3000"));
}
