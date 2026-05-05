use crate::oc::*;
use rocket_shared::types::{RequestSettings, RequestSettingValue};

pub(super) fn oc_settings_to_domain(oc: OcHttpRequestSettings) -> RequestSettings {
    RequestSettings {
        encode_url: oc.encode_url.map(inheritable_bool_to_domain),
        timeout: oc.timeout.map(inheritable_number_to_domain),
        follow_redirects: oc.follow_redirects.map(inheritable_bool_to_domain),
        max_redirects: oc.max_redirects.map(inheritable_number_to_domain),
        verify_ssl: oc.verify_ssl.map(inheritable_bool_to_domain),
    }
}

pub(super) fn domain_settings_to_oc(s: RequestSettings) -> OcHttpRequestSettings {
    OcHttpRequestSettings {
        encode_url: s.encode_url.map(domain_bool_to_inheritable),
        timeout: s.timeout.map(domain_number_to_inheritable),
        follow_redirects: s.follow_redirects.map(domain_bool_to_inheritable),
        max_redirects: s.max_redirects.map(domain_number_to_inheritable),
        verify_ssl: s.verify_ssl.map(domain_bool_to_inheritable),
    }
}

fn inheritable_bool_to_domain(ib: InheritableBoolean) -> RequestSettingValue<bool> {
    match ib {
        InheritableBoolean::Value(v) => RequestSettingValue::Value(v),
        InheritableBoolean::Inherit(s) => RequestSettingValue::Inherit(s),
    }
}

fn inheritable_number_to_domain(n: InheritableNumber) -> RequestSettingValue<f64> {
    match n {
        InheritableNumber::Value(v) => RequestSettingValue::Value(v),
        InheritableNumber::Inherit(s) => RequestSettingValue::Inherit(s),
    }
}

fn domain_bool_to_inheritable(v: RequestSettingValue<bool>) -> InheritableBoolean {
    match v {
        RequestSettingValue::Value(b) => InheritableBoolean::Value(b),
        RequestSettingValue::Inherit(s) => InheritableBoolean::Inherit(s),
    }
}

fn domain_number_to_inheritable(v: RequestSettingValue<f64>) -> InheritableNumber {
    match v {
        RequestSettingValue::Value(n) => InheritableNumber::Value(n),
        RequestSettingValue::Inherit(s) => InheritableNumber::Inherit(s),
    }
}
