use kinetic_atlas::proxy::extract_base_domain_and_subdomain;
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_extract_domain_proptest_complex(
        subdomain in "[a-zA-Z0-9_\\-]+(\\.[a-zA-Z0-9_\\-]+)*",
        name in "[a-zA-Z0-9_\\-]+",
        tld in "\\.[a-zA-Z]+"
    ) {
        let full_domain = format!("{}.{}{}", subdomain, name, tld);
        let result = extract_base_domain_and_subdomain(&full_domain, &tld);

        assert!(result.is_some());
        let (base, sub) = result.unwrap();

        assert_eq!(base, format!("{}{}", name, tld));
        assert_eq!(sub, subdomain);
    }
}
