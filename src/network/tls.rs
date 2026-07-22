use crate::config::get_services_config::SslConfig;
use crate::config::settings::MAIN_SCETY_PATH;
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use std::sync::Arc;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tracing::info;

pub fn load_manual_tls(ssl: &SslConfig) -> Result<TlsAcceptor, Box<dyn std::error::Error>> {
    let cert_path = ssl.cert.as_ref().ok_or("Missing cert path")?;
    let key_path = ssl.key.as_ref().ok_or("Missing key path")?;

    let certs = load_certs(cert_path)?;
    let key = load_private_key(key_path)?;

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}

fn load_certs(path: &str) -> Result<Vec<CertificateDer<'static>>, Box<dyn std::error::Error>> {
    let certs: Vec<_> = CertificateDer::pem_file_iter(path)?.collect::<Result<_, _>>()?;
    if certs.is_empty() {
        return Err(format!("No certificates found in {}", path).into());
    }
    info!(path=%path, count=%certs.len(), "Loaded TLS certificates");
    Ok(certs)
}

fn load_private_key(path: &str) -> Result<PrivateKeyDer<'static>, Box<dyn std::error::Error>> {
    let key = PrivateKeyDer::from_pem_file(path)?;
    info!(path=%path, "Loaded TLS private key");
    Ok(key)
}

pub fn build_acme_config(
    ssl: &SslConfig,
) -> tokio_rustls_acme::AcmeConfig<std::io::Error, std::io::Error> {
    let email = ssl.acme_email.clone().unwrap_or_default();
    let domains = ssl.acme_domains.clone().unwrap_or_default();
    let cache = ssl.acme_cache.clone().unwrap_or_else(|| {
        "{main_path}/acme-cache"
            .replace("{main_path}", MAIN_SCETY_PATH)
            .to_string()
    });
    info!(domains=?domains, email=%email, cache=%cache, "Setting up ACME");

    tokio_rustls_acme::AcmeConfig::new(domains)
        .contact_push(format!("mailto:{}", email))
        .cache(tokio_rustls_acme::caches::DirCache::new(cache))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PKCS1_RSA_KEY: &str = "-----BEGIN RSA PRIVATE KEY-----
MIIEowIBAAKCAQEAlxlzLwNSqZ2ncHGN/pUs2nTtEGfeK0tUkOAPwvYxINSTghWR
vU4v1U+3lBObn47J9DF0cdak+TbvSl+qq0jaWDPrrVK9iJWQkgoXaTYK+5PywPwX
jVaWjlC00ikrQ+UjDSqc84tD2dnvjRX5XAb8lxMbJQgYVTcudAK976wMDRFs0hMu
KzkJTrlopJIVaHiqAJ6f//2W6QGneMIEkH53rLwC/NcXqh757lcMPP9YusiXqQjT
tBc/zhrIhCTumXUzZ/jb1oRkth9xBNuv2UmtvxuteiBUve9yraGgqbzTLNereNUw
iZy5AN7hUWEK+pxbOvF9vhlT6Y48M1cpbKxEKwIDAQABAoIBABX+7MdIQDf+157F
I7rkHmkYImEurac3H8gN8LLHQgmZyHYPC8M3ZuaHj0gtaTcwrceFjNem5nx8+cKl
leO6BvGrb+swTfUHHh+UUSojZYqP/q5cmapyk2zY5NRp4n7PaWsuQTYsQiQBgh9+
C78mKTkY3UJmdkcEHG1jmiC8tphkL/xNCIvhDZluPTDq/yUj7XIZI7WxjJHwZy18
rJc3g/Jkc9IaghQNHok4LLdAL1RxbmFDF9OwG9vwgJnGflSORFBHNtMss1L+GymU
D58zPPo5CcZfO6i+wCitIVqs6H3srGood0/BR7igCGoN6MvZM+nfS+AlDOZO5F7Q
e5B5B4ECgYEAzuuTmkq4yaOEFQJB5eyceVYAco13LQPf9T1RNz2pdq8Wgs6BJ3qw
jSN6gpUhaZKZ+qDWwcOpjVmfvYF4FYhOb3eZNpVPTWIrTP2QoEe4glpZcIAr+xZd
WTcDuRuRK/Z8vzp53dRAOxWWICukhDy3C3oMYrlKasbUfWNx/fuan0ECgYEAuvBf
X2brodt1jJ14HvNkaGPQyBfNnRSFLHGPCEVHP1GEYey+9Z4RvEXfrdDYIa0qhYZz
vzxFPpGqbOHoteMUjIpEY5ChFRUJHIO/6kNcOzaD7lWeCP5raIDS2uLgbPPUo6C7
ON7v8BR9NDBhcJMtS05a8B4/kv2cjyeis7iStGsCgYEAww7oMcbGs65lULi0Dl8i
km80NMiO0+yXLsQCz6RdH/ilq+GnduP9ks8jKf4TZUZByTXdvQMJzqnyH97wqLu5
1PJViFLwUu58CzPtJmr10EwDjD4HN8c5cGSgKduG2n6d0lb5ktgHRKtwvhrmF5J0
q2j+TAKH2Ghe32TjjJ2mgQECgYBrWcw7Gfxot4Fanbc0dusLM37a2Sh/cyBC9HeB
9V7D0skl/vFuVTa0GqAnzc3AERRhF2Pyxuaw1q+61URw5xWO23wIfS6zz5+q21Hj
colNi7HZtRsK6Se/HHN5tV3R03gh+xRoxUWeZfW8eagLIMma/EUmrQgvHirA3q8F
bBH7PwKBgC/svCisKtczlbL/7Fdq8YNLsVrHttjIRsXU8qI3pM3axQPiqNOE4K1X
j4Q482rLe30zLpqTGPuh+7gJWjNHxZa2H4wGHmdKAjFgXSkoIwhIBIiEb0iGkugz
m256uF9A6DqHzck/k4xLMQHLAmh7D9d++/+6HJed/gklxvpcn2zw
-----END RSA PRIVATE KEY-----
";

    const PKCS8_KEY: &str = "-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCXGXMvA1Kpnadw
cY3+lSzadO0QZ94rS1SQ4A/C9jEg1JOCFZG9Ti/VT7eUE5ufjsn0MXRx1qT5Nu9K
X6qrSNpYM+utUr2IlZCSChdpNgr7k/LA/BeNVpaOULTSKStD5SMNKpzzi0PZ2e+N
FflcBvyXExslCBhVNy50Ar3vrAwNEWzSEy4rOQlOuWikkhVoeKoAnp///ZbpAad4
wgSQfnesvAL81xeqHvnuVww8/1i6yJepCNO0Fz/OGsiEJO6ZdTNn+NvWhGS2H3EE
26/ZSa2/G616IFS973KtoaCpvNMs16t41TCJnLkA3uFRYQr6nFs68X2+GVPpjjwz
VylsrEQrAgMBAAECggEAFf7sx0hAN/7XnsUjuuQeaRgiYS6tpzcfyA3wssdCCZnI
dg8Lwzdm5oePSC1pNzCtx4WM16bmfHz5wqWV47oG8atv6zBN9QceH5RRKiNlio/+
rlyZqnKTbNjk1Gnifs9pay5BNixCJAGCH34LvyYpORjdQmZ2RwQcbWOaILy2mGQv
/E0Ii+ENmW49MOr/JSPtchkjtbGMkfBnLXyslzeD8mRz0hqCFA0eiTgst0AvVHFu
YUMX07Ab2/CAmcZ+VI5EUEc20yyzUv4bKZQPnzM8+jkJxl87qL7AKK0hWqzofeys
aih3T8FHuKAIag3oy9kz6d9L4CUM5k7kXtB7kHkHgQKBgQDO65OaSrjJo4QVAkHl
7Jx5VgByjXctA9/1PVE3Pal2rxaCzoEnerCNI3qClSFpkpn6oNbBw6mNWZ+9gXgV
iE5vd5k2lU9NYitM/ZCgR7iCWllwgCv7Fl1ZNwO5G5Er9ny/Onnd1EA7FZYgK6SE
PLcLegxiuUpqxtR9Y3H9+5qfQQKBgQC68F9fZuuh23WMnXge82RoY9DIF82dFIUs
cY8IRUc/UYRh7L71nhG8Rd+t0NghrSqFhnO/PEU+kaps4ei14xSMikRjkKEVFQkc
g7/qQ1w7NoPuVZ4I/mtogNLa4uBs89SjoLs43u/wFH00MGFwky1LTlrwHj+S/ZyP
J6KzuJK0awKBgQDDDugxxsazrmVQuLQOXyKSbzQ0yI7T7JcuxALPpF0f+KWr4ad2
4/2SzyMp/hNlRkHJNd29AwnOqfIf3vCou7nU8lWIUvBS7nwLM+0mavXQTAOMPgc3
xzlwZKAp24bafp3SVvmS2AdEq3C+GuYXknSraP5MAofYaF7fZOOMnaaBAQKBgGtZ
zDsZ/Gi3gVqdtzR26wszftrZKH9zIEL0d4H1XsPSySX+8W5VNrQaoCfNzcARFGEX
Y/LG5rDWr7rVRHDnFY7bfAh9LrPPn6rbUeNyiU2Lsdm1GwrpJ78cc3m1XdHTeCH7
FGjFRZ5l9bx5qAsgyZr8RSatCC8eKsDerwVsEfs/AoGAL+y8KKwq1zOVsv/sV2rx
g0uxWse22MhGxdTyojekzdrFA+Ko04TgrVePhDjzast7fTMumpMY+6H7uAlaM0fF
lrYfjAYeZ0oCMWBdKSgjCEgEiIRvSIaS6DObbnq4X0DoOofNyT+TjEsxAcsCaHsP
1377/7ocl53+CSXG+lyfbPA=
-----END PRIVATE KEY-----
";

    struct TempPemFile {
        path: std::path::PathBuf,
    }

    impl TempPemFile {
        fn new(tag: &str, contents: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "scety_test_{}_{}_{}.pem",
                std::process::id(),
                tag,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::write(&path, contents).expect("failed to write temp key file");
            Self { path }
        }

        fn path_str(&self) -> &str {
            self.path.to_str().unwrap()
        }
    }

    impl Drop for TempPemFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    #[test]
    fn loads_pkcs1_rsa_key() {
        let file = TempPemFile::new("pkcs1", PKCS1_RSA_KEY);
        let key = load_private_key(file.path_str());
        assert!(
            key.is_ok(),
            "expected classic PKCS1 (\"BEGIN RSA PRIVATE KEY\") to parse: {:?}",
            key.err()
        );
    }

    #[test]
    fn loads_pkcs8_key() {
        let file = TempPemFile::new("pkcs8", PKCS8_KEY);
        let key = load_private_key(file.path_str());
        assert!(
            key.is_ok(),
            "expected PKCS8 key to still parse: {:?}",
            key.err()
        );
    }

    #[test]
    fn rejects_empty_key_file() {
        let file = TempPemFile::new("empty", "");
        let key = load_private_key(file.path_str());
        assert!(key.is_err(), "empty file should not produce a key");
    }
}
