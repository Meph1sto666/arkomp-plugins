use crate::skin::{AnimationTransition, GeneralOperatorSkin};
use crate::texture::SpineTexture;
use egui::{self, Color32, Context, Pos2, Stroke, Ui};
use serde_json::json;
use shared::{
    self,
    ipc::{Response, events::Event},
    operator::{Error, Operator},
    skin::OperatorSkin,
};
use tracing::debug;
use wgpu::Color;

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
    mouse_pos: Option<Pos2>,
    selected: bool,
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
            mouse_pos: None,
            selected: false,
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
        (self.walking_speed / (self.fps) as f32) * self.scale
    }

    // fn is_hovered(&self, renderable: &rusty_spine::controller::SkeletonRenderable) -> bool {
    //     renderable.vertices
    // }
    fn reset_interaction(&mut self) {
        self.start_animation("Relax").unwrap();
    }
}

impl Operator for GeneralOperator {
    fn render(&mut self, ctx: &Context, ui: &mut Ui) {
        // save mouse position
        self.mouse_pos = ui.input(|i| i.pointer.hover_pos());

        let controller: &mut rusty_spine::controller::SkeletonController =
            self.skin.get_active_controller_mut();

        let mut hovered = false;
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

            let bound = mesh.calc_bounds();
            let _colour: egui::Color32 = if let Some(mouse_pos) = self.mouse_pos
                && bound.contains(mouse_pos)
            {
                hovered = true;
                egui::Color32::CYAN
            } else if self.selected {
                egui::Color32::LIGHT_GREEN
            } else {
                egui::Color32::MAGENTA
            };

            // ui.painter().rect_stroke(
            //     bound,
            //     0,
            //     egui::Stroke::new(1.0, colour),
            //     egui::StrokeKind::Outside,
            // );

            ui.painter().add(mesh);
        }

        if ui.input(|i| i.pointer.primary_down()) && hovered {
            self.position += ui.input(|i| i.pointer.delta());
            self.destiny = self.position;
        } else if ui.input(|i| i.pointer.secondary_clicked()) && hovered {
            self.selected = !self.selected;
            // self.destiny += ui.input(|i| i.pointer.delta())
        } else if let Some(pos) = ui.input(|i| i.pointer.interact_pos())
            && self.selected
            && ui.input(|i| i.pointer.secondary_clicked())
        {
            self.destiny = pos;
        }
        // unsafe dogshit, redo this properly lol
        if hovered && ui.input(|i| i.pointer.primary_clicked()) {
            if let Some(mouse_pos) = self.mouse_pos {
                ui.painter().circle(
                    mouse_pos,
                    2.0,
                    Color32::GREEN,
                    Stroke::new(0.0, Color32::TRANSPARENT),
                );
            }
            self.start_animation("Interact").unwrap();

            let self_ptr = self as *mut Self;
            let prev_ani = self.skin.previous_ani.clone();

            self.skin
                .dorm_variant
                .controller
                .animation_state
                .set_listener(move |_, e| match e {
                    rusty_spine::AnimationEvent::End { .. }
                    | rusty_spine::AnimationEvent::Complete { .. } => unsafe {
                        let mut_self = &mut *self_ptr;
                        mut_self.start_animation(&prev_ani).ok();
                    },
                    _ => {}
                });
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
            ctx.request_repaint();
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
            // Event::OnMouseMove { position, .. } => self.mouse_pos = *position,
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
