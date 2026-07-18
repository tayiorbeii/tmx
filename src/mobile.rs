use crate::model::UiProfile;

#[derive(Debug, Clone, Default)]
pub struct ProfileInputs {
    pub explicit_desktop: bool,
    pub explicit_mobile: bool,
    pub env_tmx_ui: Option<String>,
    pub config_default_ui: String,
    pub mobile_width_threshold: u16,
    pub mobile_height_threshold: u16,
    pub client_size: Option<(u16, u16)>,
}

pub fn select_profile(inputs: &ProfileInputs) -> UiProfile {
    if inputs.explicit_desktop {
        return UiProfile::Desktop;
    }
    if inputs.explicit_mobile {
        return UiProfile::Mobile;
    }
    if let Some(env) = &inputs.env_tmx_ui {
        match env.as_str() {
            "desktop" => return UiProfile::Desktop,
            "mobile" => return UiProfile::Mobile,
            _ => {}
        }
    }
    match inputs.config_default_ui.as_str() {
        "desktop" => return UiProfile::Desktop,
        "mobile" => return UiProfile::Mobile,
        _ => {}
    }
    if let Some((w, h)) = inputs.client_size {
        let width_threshold = inputs.mobile_width_threshold.max(1);
        let height_threshold = inputs.mobile_height_threshold.max(1);
        if w < width_threshold || h < height_threshold {
            return UiProfile::Mobile;
        }
    }
    UiProfile::Desktop
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_wins() {
        let p = select_profile(&ProfileInputs {
            explicit_mobile: true,
            config_default_ui: "desktop".into(),
            ..Default::default()
        });
        assert_eq!(p, UiProfile::Mobile);
    }

    #[test]
    fn env_wins_over_config() {
        let p = select_profile(&ProfileInputs {
            env_tmx_ui: Some("mobile".into()),
            config_default_ui: "desktop".into(),
            ..Default::default()
        });
        assert_eq!(p, UiProfile::Mobile);
    }

    #[test]
    fn small_client_is_mobile() {
        let p = select_profile(&ProfileInputs {
            config_default_ui: "auto".into(),
            mobile_width_threshold: 100,
            mobile_height_threshold: 35,
            client_size: Some((80, 40)),
            ..Default::default()
        });
        assert_eq!(p, UiProfile::Mobile);
    }
}
