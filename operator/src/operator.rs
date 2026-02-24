use crate::skin::{AnimationTransition, GeneralOperatorSkin};
use egui::{self, Context, Pos2, Ui};
use serde_json::json;
use shared::{
    self,
    ipc::{Response, events::Event},
    operator::{Error, Operator},
    skin::OperatorSkin,
    texture::SpineTexture,
};
use tracing::debug;

#[derive(Debug, Default, Clone, Copy)]
enum FacingDirection {
    #[default]
    Right = 1,
    Left = -1,
}

#[derive(Debug)]
pub struct GeneralOperator {
    id: String,
    event_tx: tokio::sync::mpsc::Sender<Event>,
    skin: GeneralOperatorSkin,
    last_update_time: f64,

    scale: f32,
    position: Pos2,
    destiny: Pos2,
    facing: FacingDirection,
    fps: u8,
    walking_speed: f32,
}

impl GeneralOperator {
    pub fn new(
        op_id: &str,
        event_tx: tokio::sync::mpsc::Sender<Event>,
    ) -> Result<Box<GeneralOperator>, Error> {
        Ok(Box::new(Self {
            id: op_id.to_string(),
            event_tx,
            skin: GeneralOperatorSkin::new(op_id, None)?,
            last_update_time: chrono::Utc::now().timestamp() as f64,
            position: Pos2::new(0.0, 0.0),
            scale: 0.75,
            destiny: Pos2::new(0.0, 0.0),
            facing: FacingDirection::Right,
            fps: 60,
            walking_speed: 200.0,
        }))
    }

    fn move_to(&mut self, pos: Pos2) {
        let velocity = self.velocity();
        if self.position == pos {
            return;
        }
        if self.position.distance_sq(pos) < velocity.powi(2) {
            self.position = pos
        }
        if self.position.x > pos.x {
            self.facing = FacingDirection::Left
        } else {
            self.facing = FacingDirection::Right
        }
        self.position += egui::vec2(
            if self.position.x > pos.x {
                -(velocity)
            } else if (self.position.x - pos.x).abs() < velocity {
                0.0
            } else {
                velocity
            },
            if self.position.y > pos.y {
                -(velocity)
            } else if (self.position.y - pos.y).abs() < velocity {
                0.0
            } else {
                velocity
            },
        );
    }

    fn walk_to(&mut self, pos: Pos2) {
        self.start_animation("Move").unwrap();
        self.move_to(pos);
    }

    fn velocity(&self) -> f32 {
        (self.walking_speed as f32 / (self.fps) as f32) * self.scale
    }
}

impl Operator for GeneralOperator {
    fn render(&mut self, ctx: &Context, ui: &mut Ui) {
        let controller: &mut rusty_spine::controller::SkeletonController =
            self.skin.get_active_controller_mut();

        for renderable in controller.renderables() {
            let mut texture_id: egui::TextureId = egui::TextureId::default();
            if let Some(attachment_renderer_object) = renderable.attachment_renderer_object {
                let texture: &mut SpineTexture =
                    unsafe { &mut *(attachment_renderer_object as *mut SpineTexture) };
                match texture {
                    SpineTexture::Pending { .. } => {
                        self.skin.ensure_textures_loaded(ctx);
                    }
                    SpineTexture::Loaded(texture_handle) => {
                        texture_id = texture_handle.id();
                    }
                }
            }

            let mut mesh: egui::Mesh = egui::Mesh::with_texture(texture_id);
            let vertex_count: usize = renderable.vertices.len();

            for i in 0..vertex_count {
                let [x, y] = renderable.vertices[i];
                let [u, v] = renderable.uvs[i];

                let pos = Pos2::new(
                    (0 as f32) + self.position.x + (((self.facing as i8) as f32) * x * self.scale),
                    self.position.y - (y * self.scale),
                );

                mesh.vertices.push(egui::epaint::Vertex {
                    pos,
                    uv: egui::pos2(u, v),
                    color: egui::Color32::WHITE,
                });
            }

            for idx in renderable.indices.chunks_exact(3) {
                mesh.add_triangle(idx[0] as u32, idx[1] as u32, idx[2] as u32);
            }
            ui.painter().add(mesh);
        }
    }

    fn update_animation(&mut self, ctx: &Context) {
        let now = ctx.input(|i| i.time);
        let delta = (now - self.last_update_time) as f32;
        self.last_update_time = now;

        self.skin
            .get_active_controller_mut()
            .update(delta.max(1.0 / 240.0));
        self.skin.apply_animation_state_change();
        if self.destiny != self.position {
            self.walk_to(self.destiny);
            if self.destiny == self.position {
                let prev_ani = &self.skin.previous_ani.clone();
                let _ = self.skin.set_animation(
                    prev_ani,
                    0,
                    AnimationTransition::new(prev_ani.into(), 0.2),
                    false,
                );
            }
        }
    }

    fn id(&self) -> String {
        self.id.clone()
    }

    fn start_animation(&mut self, anim: &str) -> Result<(), shared::operator::Error> {
        self.skin.set_animation(
            anim,
            0,
            crate::skin::AnimationTransition::new(anim.to_string(), 0.2),
            false,
        )?;
        self.event_tx
            .try_send(Event::OnAnimationChange {
                to: "broadcast".to_string(),
                from: self.id.clone(),
                ani: anim.to_string(),
            })
            .expect("Failed to send event");
        Ok(())
    }

    fn load_textures(&mut self, ctx: &egui::Context) {
        self.skin.ensure_textures_loaded(ctx);
    }

    fn event_handler(&mut self, event: shared::ipc::events::Event) -> Result<Response, Error> {
        debug!("handling event {} for {:?}", self.id, event);
        match &event {
            Event::OnRetreat { .. } => return Ok(Response::Error("not implemented".to_string())),
            Event::SetSkin { .. } => return Ok(Response::Error("not implemented".to_string())),
            Event::SetAnimation { ani, .. } => self.start_animation(ani)?,
            Event::MoveTo { position, .. } => self.destiny = Pos2::new(position.0, position.1),
            Event::Resize { scale, .. } => self.scale = *scale,
            Event::SetFacingDirection { direction, .. } => {
                self.facing = if *direction == "right" {
                    FacingDirection::Right
                } else {
                    FacingDirection::Left
                }
            }
            Event::SetPosition { position, .. } => {
                self.destiny = Pos2::new(position.0, position.1);
                self.position = Pos2::new(position.0, position.1)
            }

            Event::CustomEvent { .. } => {}
            _ => {
                return Ok(Response::Error(format!(
                    "Event {:?} not implemented",
                    event
                )));
            }
        };
        Ok(Response::Success(
            json!({
                "operator": event.to()
            })
            .to_string(),
        ))
    }

    fn infos(&self) -> String {
        json!({
            "id": self.id,
            "animations": self.skin.animations()
        })
        .to_string()
    }
}
