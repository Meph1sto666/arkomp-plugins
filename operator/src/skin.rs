use crate::texture::SpineTexture;
use rusty_spine::{
    AnimationStateData, Atlas, Skeleton, SkeletonBinary, SkeletonData,
    controller::SkeletonController,
};
use shared::skin::{Error, OperatorSkin, SkeletonFile};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tracing::{debug, error, warn};

trait Spine {
    fn atlas(base_path: PathBuf) -> Result<Arc<Atlas>, Error> {
        let atlas: Arc<Atlas> = Arc::new(
            Atlas::new_from_file(base_path.with_extension("atlas")).map_err(
                |e: rusty_spine::SpineError| {
                    error!("loading atlas failed: {}", e);
                    Error::AtlasFileNotFound(e.to_string())
                },
            )?,
        );
        Ok(atlas)
    }

    fn skel_data(base_path: PathBuf) -> Result<Arc<SkeletonData>, Error> {
        debug!(
            "Loading {}",
            base_path.with_extension("skel").to_string_lossy()
        );
        let skeleton_file = SkeletonFile::Binary(base_path.with_extension("skel"));
        Ok(Arc::new(match skeleton_file {
            SkeletonFile::Binary(path) => SkeletonBinary::new(Self::atlas(base_path)?)
                .read_skeleton_data_file(path)
                .map_err(|e| {
                    error!("loading skeleton data failed: {}", e);
                    Error::SkeletonFileNotFound(e.to_string())
                })?,
            SkeletonFile::Json(path) => SkeletonBinary::new(Self::atlas(base_path)?)
                .read_skeleton_data_file(path)
                .map_err(|e| {
                    error!("loading skeleton data failed: {}", e);
                    Error::SkeletonFileNotFound(e.to_string())
                })?,
        }))
    }

    fn state_data(skeleton_data: Arc<SkeletonData>) -> Arc<AnimationStateData> {
        Arc::new(AnimationStateData::new(skeleton_data.clone()))
    }
}

#[derive(Debug)]
pub struct DormSpine {
    pub(crate) skeleton: Skeleton,
    pub(crate) controller: SkeletonController,
}
impl Spine for DormSpine {}
impl DormSpine {
    fn new(base_path: PathBuf) -> Result<Self, Error> {
        debug!("Loading dorm spine {}", base_path.to_string_lossy());
        let skeleton_data = Self::skel_data(base_path)?;
        Ok(Self {
            skeleton: Skeleton::new(skeleton_data.clone()),
            controller: SkeletonController::new(
                skeleton_data.clone(),
                Self::state_data(skeleton_data),
            ),
        })
    }
}

#[derive(Debug)]
pub struct BattleSpine {
    pub(crate) skeleton: Skeleton,
    pub(crate) controller: SkeletonController,
}
impl Spine for BattleSpine {}
impl BattleSpine {
    fn new(base_path: PathBuf) -> Result<Self, Error> {
        debug!("Loading battle spine {}", base_path.to_string_lossy());
        debug!("Loaded skeleton data {}", base_path.to_string_lossy());
        let skeleton_data = Self::skel_data(base_path.clone())?;
        let ret = Ok(Self {
            skeleton: Skeleton::new(skeleton_data.clone()),
            controller: SkeletonController::new(
                skeleton_data.clone(),
                Self::state_data(skeleton_data),
            ),
        });
        debug!("Loaded battle spine {}", base_path.to_string_lossy());
        ret
    }
}

#[derive(Debug)]
pub enum SkinVariant {
    Dorm,
    Battle,
}

#[derive(Debug)]
pub(crate) struct AnimationTransition {
    to: String,
    duration: f32,
}
impl Default for AnimationTransition {
    fn default() -> Self {
        Self {
            to: "Default".into(),
            duration: 0.2,
        }
    }
}
impl AnimationTransition {
    pub fn new(to: String, duration: f32) -> Self {
        Self {
            to: to,
            duration: duration,
        }
    }
}

#[derive(Debug)]
pub struct GeneralOperatorSkin {
    pub(crate) operator_id: String,
    pub(crate) id: Option<String>,
    pub(crate) dorm_variant: DormSpine,
    pub(crate) battle_variant: Option<BattleSpine>,
    pub(crate) active_skin_variant: SkinVariant,
    pub(crate) animation_mapping: HashMap<String, SkinVariant>,
    pub(crate) active_ani: String,
    pub(crate) previous_ani: String,
}

impl GeneralOperatorSkin {
    pub(crate) fn get_active_controller_mut(&mut self) -> &mut SkeletonController {
        match self.active_skin_variant {
            SkinVariant::Dorm => &mut self.dorm_variant.controller,
            // SkinVariant::Battle => &mut self.battle_variant.controller,
            SkinVariant::Battle => panic!(),
        }
    }

    // pub(crate) fn get_active_controller(&self) -> &SkeletonController {
    //     match self.active_skin_variant {
    //         SkinVariant::Dorm => &self.dorm_variant.controller,
    //         _ => panic!()
    //         // SkinVariant::Battle => &self.battle_variant.controller,
    //     }
    // }

    // pub(crate) fn set_active_controller(&mut self, variant: SkinVariant) {
    //     self.active_skin_variant = variant
    // }

    pub(crate) fn apply_animation_state_change(&mut self) {
        match self.active_skin_variant {
            SkinVariant::Dorm => self
                .dorm_variant
                .controller
                .animation_state
                .apply(&mut self.dorm_variant.skeleton),
            SkinVariant::Battle => {
                if let Some(battle_variant) = self.battle_variant.as_mut() {
                    let controller = &battle_variant.controller;
                    controller
                        .animation_state
                        .apply(&mut battle_variant.skeleton)
                } else {
                    false
                }
            }
        };
    }

    pub(crate) fn animations(&self) -> Vec<String> {
        let mut anis: Vec<String> = self
            .dorm_variant
            .skeleton
            .data()
            .animations()
            .map(|f| f.name().to_string())
            .collect();
        if self.battle_variant.is_some() {
            anis.append(
                &mut self
                    .battle_variant
                    .as_ref()
                    .unwrap()
                    .skeleton
                    .data()
                    .animations()
                    .into_iter()
                    .map(|f| f.name().to_string())
                    .collect(),
            );
        }
        anis
    }

    pub(crate) fn skin_variant_from_animation(
        &self,
        animation: &str,
    ) -> Result<SkinVariant, Error> {
        Ok(
            match self
                .animation_mapping
                .get(animation)
                .ok_or(Error::AnimationNotFound(format!(
                    "\"animation\": {animation}"
                )))? {
                SkinVariant::Dorm => SkinVariant::Dorm,
                SkinVariant::Battle => SkinVariant::Battle,
            },
        )
    }

    pub(crate) fn clear_animations(&mut self) {
        self.dorm_variant.controller.animation_state.clear_tracks();
        if let Some(bs) = &mut self.battle_variant {
            bs.controller.animation_state.clear_tracks();
        }
    }

    // pub(crate) fn get_current_animation(
    //     &self,
    //     track_index: usize,
    // ) -> Result<rusty_spine::c_interface::CTmpRef<'_, AnimationState, rusty_spine::TrackEntry>, Error>
    // {
    //     let a = self
    //         .get_active_controller()
    //         .animation_state
    //         .get_current(track_index)
    //         .ok_or(Error::NoActiveAnimation(format!(
    //             "\"track_index\": {}",
    //             track_index
    //         )))?;
    //     Ok(a)
    // }

    pub(crate) fn set_animation(
        &mut self,
        ani: &str,
        track: usize,
        ani_transition: AnimationTransition,
        force: bool,
    ) -> Result<(), Error> {
        self.active_skin_variant = self.skin_variant_from_animation(&ani)?;
        let from = self.active_ani.clone();

        if self.active_ani == ani && !force {
            return Ok(());
        }

        if force {
            self.clear_animations();
        } else {
            self.get_active_controller_mut()
                .animation_state
                .data_mut()
                .set_mix_by_name(&from, &ani_transition.to, ani_transition.duration);
        }

        self.get_active_controller_mut()
            .animation_state
            .set_animation_by_name(track, ani, true)
            .unwrap();
        self.previous_ani = self.active_ani.clone();
        self.active_ani = ani.to_string();
        Ok(())
    }
}

impl OperatorSkin for GeneralOperatorSkin {
    fn new(operator_id: &str, skin_id: Option<&str>) -> Result<Self, Error>
    where
        Self: Sized,
    {
        let char_skin_id = if skin_id.is_some() {
            &format!("{operator_id}_{}", skin_id.unwrap())
        } else {
            operator_id
        };
        let dorm_files_base_path = PathBuf::from(format!(
            "assets/dyn/building/vault/characters/build_{char_skin_id}/build_{char_skin_id}"
        ));

        let battle_files_base_path = PathBuf::from(format!(
            "assets/dyn/battle/prefabs/skins/character/{operator_id}/{}/{char_skin_id}",
            if skin_id.is_some() {
                char_skin_id
            } else {
                skin_id.unwrap_or("defaultskin")
            }
        ));

        // let ds = hw_exception::catch(|| DormSpine::new(dorm_files_base_path)).unwrap()?;
        let ds = DormSpine::new(dorm_files_base_path).unwrap();
        // let bs = hw_exception::catch(|| BattleSpine::new(battle_files_base_path).ok()).ok().flatten();
        let bs: Option<BattleSpine> = None;

        let mut ds_anis: Vec<(String, SkinVariant)> = ds
            .skeleton
            .data()
            .animations()
            .into_iter()
            .map(|f| (f.name().to_string(), SkinVariant::Dorm))
            .collect();
        if let Some(bs) = &bs {
            ds_anis.append(
                &mut bs
                    .skeleton
                    .data()
                    .animations()
                    .into_iter()
                    .map(|f| (f.name().to_string(), SkinVariant::Battle))
                    .collect(),
            );
        }
        debug!(
            "Skin {} for {} loaded",
            skin_id.unwrap_or("None"),
            operator_id
        );
        Ok(Self {
            operator_id: operator_id.to_string(),
            id: skin_id.map(|f| f.into()),
            animation_mapping: HashMap::from_iter(ds_anis),
            dorm_variant: ds,
            battle_variant: bs,
            active_skin_variant: SkinVariant::Dorm,
            active_ani: "Default".to_string(),
            previous_ani: "Default".to_string(),
        })
    }

    fn operator_id(&self) -> String {
        self.operator_id.clone()
    }

    fn id(&self) -> Option<String> {
        self.id.clone()
    }

    fn ensure_textures_loaded(&mut self, ctx: &egui::Context) {
        let mut processed_textures: std::collections::HashSet<*const rusty_spine::c::c_void> =
            std::collections::HashSet::new();

        for controller in vec![
            // &mut self.battle_variant.controller,
            &mut self.dorm_variant.controller,
        ] {
            for renderable in controller.renderables() {
                let attachment_ptr = match renderable.attachment_renderer_object {
                    Some(ptr) => ptr,
                    None => continue,
                };

                if processed_textures.contains(&attachment_ptr) {
                    continue;
                }
                processed_textures.insert(attachment_ptr);

                let texture_state: &mut SpineTexture =
                    unsafe { &mut *(attachment_ptr as *mut SpineTexture) };

                match texture_state {
                    SpineTexture::Pending {
                        path,
                        min_filter,
                        mag_filter,
                        x_wrap,
                        y_wrap: _,
                        format: _,
                    } => {
                        debug!("Loading texture: {}", path.clone());
                        match image::open(path.clone()) {
                            Ok(image) => {
                                let rgba: image::ImageBuffer<image::Rgba<u8>, Vec<u8>> =
                                    image.to_rgba8();
                                let size: [usize; 2] =
                                    [rgba.width() as usize, rgba.height() as usize];
                                let texture_handle = ctx.load_texture(
                                    path.clone(),
                                    egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()),
                                    egui::TextureOptions {
                                        magnification: *mag_filter,
                                        minification: *min_filter,
                                        wrap_mode: *x_wrap,
                                        mipmap_mode: None,
                                    },
                                );
                                // Store the TextureHandle to keep the texture alive
                                *texture_state = SpineTexture::Loaded(texture_handle);
                            }
                            Err(e) => warn!("Failed to load texture {}: {}", path, e),
                        }
                    }
                    SpineTexture::Loaded(_) => {
                        // Already loaded, do nothing
                    }
                }
            }
        }
    }
}
