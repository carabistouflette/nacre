//! Exact generated-C dependency markers and their trusted compiler arguments.
//! Selection is closed over prerequisites before exposing flags in catalog order.
#[derive(Clone, Copy)]
#[repr(u8)]
enum NativeLibrary {
    Ssl,
    Crypto,
    Z,
    Gmp,
    Curl,
    Icu,
    Archive,
    Xslt,
    Xml,
    Sqlite,
    Uri,
    Pcre2,
}

#[derive(Clone, Copy)]
pub(super) struct NativeLibraries(u16);

impl NativeLibraries {
    pub(super) fn from_source(source: &str) -> Self {
        let mut libraries = Self(0);
        for line in source.lines() {
            if !line.starts_with("/* nacre-link: ") { continue; }
            if let Some(dependency) = NATIVE_DEPENDENCIES.iter().find(|dependency| line == dependency.marker) {
                libraries.insert(dependency.library);
            }
        }
        loop {
            let mut added = false;
            for dependency in &NATIVE_DEPENDENCIES {
                if libraries.contains(dependency.library) {
                    for required in dependency.requires {
                        added |= libraries.insert(*required);
                    }
                }
            }
            if !added { return libraries; }
        }
    }

    pub(super) fn link_flags(self) -> impl Iterator<Item = &'static str> {
        NATIVE_DEPENDENCIES.iter()
            .filter(move |dependency| self.contains(dependency.library))
            .flat_map(|dependency| dependency.link_flags.iter().copied())
    }

    pub(super) fn header_flags(self) -> impl Iterator<Item = &'static str> {
        NATIVE_DEPENDENCIES.iter()
            .filter(move |dependency| self.contains(dependency.library))
            .flat_map(|dependency| dependency.header_flags.iter().copied())
    }

    fn contains(self, library: NativeLibrary) -> bool {
        self.0 & (1_u16 << library as u8) != 0
    }

    fn insert(&mut self, library: NativeLibrary) -> bool {
        let bit = 1_u16 << library as u8;
        let added = self.0 & bit == 0;
        self.0 |= bit;
        added
    }
}

struct NativeDependency {
    library: NativeLibrary,
    marker: &'static str,
    requires: &'static [NativeLibrary],
    link_flags: &'static [&'static str],
    header_flags: &'static [&'static str],
}

// Catalog order is the stable linker argument order.
static NATIVE_DEPENDENCIES: [NativeDependency; 12] = [
    NativeDependency {
        library: NativeLibrary::Ssl, marker: "/* nacre-link: ssl */",
        requires: &[NativeLibrary::Crypto], link_flags: &["-lssl"], header_flags: &[],
    },
    NativeDependency {
        library: NativeLibrary::Crypto, marker: "/* nacre-link: crypto */",
        requires: &[], link_flags: &["-lcrypto"], header_flags: &[],
    },
    NativeDependency {
        library: NativeLibrary::Z, marker: "/* nacre-link: z */",
        requires: &[], link_flags: &["-lz"], header_flags: &[],
    },
    NativeDependency {
        library: NativeLibrary::Gmp, marker: "/* nacre-link: gmp */",
        requires: &[], link_flags: &["-lgmp"], header_flags: &[],
    },
    NativeDependency {
        library: NativeLibrary::Curl, marker: "/* nacre-link: curl */",
        requires: &[], link_flags: &["-lcurl"], header_flags: &[],
    },
    NativeDependency {
        library: NativeLibrary::Icu, marker: "/* nacre-link: icu */",
        requires: &[], link_flags: &["-licuuc", "-licui18n"], header_flags: &[],
    },
    NativeDependency {
        library: NativeLibrary::Archive, marker: "/* nacre-link: archive */",
        requires: &[], link_flags: &["-larchive"], header_flags: &[],
    },
    NativeDependency {
        library: NativeLibrary::Xslt, marker: "/* nacre-link: xslt */",
        requires: &[NativeLibrary::Xml], link_flags: &["-lxslt"], header_flags: &[],
    },
    NativeDependency {
        library: NativeLibrary::Xml, marker: "/* nacre-link: xml */",
        requires: &[], link_flags: &["-lxml2"], header_flags: &["-I/usr/include/libxml2"],
    },
    NativeDependency {
        library: NativeLibrary::Sqlite, marker: "/* nacre-link: sqlite */",
        requires: &[], link_flags: &["-lsqlite3"], header_flags: &[],
    },
    NativeDependency {
        library: NativeLibrary::Uri, marker: "/* nacre-link: uri */",
        requires: &[], link_flags: &["-luriparser"], header_flags: &[],
    },
    NativeDependency {
        library: NativeLibrary::Pcre2, marker: "/* nacre-link: pcre2 */",
        requires: &[], link_flags: &["-lpcre2-8"], header_flags: &[],
    },
];
