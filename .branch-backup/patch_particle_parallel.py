# -*- coding: utf-8 -*-
import io
p = 'prpr/src/particle.rs'
c = io.open(p, 'r', encoding='utf-8').read()

old = """        if !self.config.gpu_driven {
        for (gpu, cpu) in self.gpu_particles.iter_mut().zip(&mut self.cpu_counterpart) {
            // TODO: this is not quite the way to apply acceleration, this is not
            // fps independent and just wrong
            cpu.velocity += cpu.velocity * self.config.linear_accel * dt;
            cpu.angular_velocity += cpu.angular_velocity * self.config.angular_accel * dt;
            cpu.angular_velocity *= 1.0 - self.config.angular_damping;

            gpu.color = {
                let t = cpu.lived / cpu.lifetime;
                if t < 0.5 {
                    let t = t * 2.;
                    self.config.colors_curve.start.to_vec() * (1.0 - t) + self.config.colors_curve.mid.to_vec() * t
                } else {
                    let t = (t - 0.5) * 2.;
                    self.config.colors_curve.mid.to_vec() * (1.0 - t) + self.config.colors_curve.end.to_vec() * t
                }
            };
            gpu.color *= cpu.color.to_vec();
            gpu.pos += vec4(cpu.velocity.x, cpu.velocity.y, cpu.angular_velocity, 0.0) * dt;

            gpu.pos.w = cpu.initial_size * self.batched_size_curve.as_ref().map_or(1.0, |curve| curve.get(cpu.lived / cpu.lifetime));

            if cpu.lifetime != 0.0 {
                gpu.data.y = cpu.lived / cpu.lifetime;
            }

            //cpu.lived = f32::min(cpu.lived + dt, cpu.lifetime);
            cpu.lived += dt;
            cpu.velocity += self.config.gravity * dt;

            if let Some(atlas) = &self.config.atlas {
                if cpu.lifetime != 0.0 {
                    cpu.frame = (cpu.lived / cpu.lifetime * (atlas.end_index - atlas.start_index) as f32) as u16 + atlas.start_index;
                }

                let x = cpu.frame % atlas.n;
                let y = cpu.frame / atlas.n;

                gpu.uv = vec4(x as f32 / atlas.n as f32, y as f32 / atlas.m as f32, 1.0 / atlas.n as f32, 1.0 / atlas.m as f32);
            } else {
                gpu.uv = vec4(0.0, 0.0, 1.0, 1.0);
            }
        }
        }"""

new = """        if !self.config.gpu_driven {
            use rayon::prelude::*;
            self.gpu_particles
                .par_iter_mut()
                .zip(self.cpu_counterpart.par_iter_mut())
                .for_each(|(gpu, cpu)| {
                    // TODO: this is not quite the way to apply acceleration, this is not
                    // fps independent and just wrong
                    cpu.velocity += cpu.velocity * self.config.linear_accel * dt;
                    cpu.angular_velocity += cpu.angular_velocity * self.config.angular_accel * dt;
                    cpu.angular_velocity *= 1.0 - self.config.angular_damping;

                    gpu.color = {
                        let t = cpu.lived / cpu.lifetime;
                        if t < 0.5 {
                            let t = t * 2.;
                            self.config.colors_curve.start.to_vec() * (1.0 - t) + self.config.colors_curve.mid.to_vec() * t
                        } else {
                            let t = (t - 0.5) * 2.;
                            self.config.colors_curve.mid.to_vec() * (1.0 - t) + self.config.colors_curve.end.to_vec() * t
                        }
                    };
                    gpu.color *= cpu.color.to_vec();
                    gpu.pos += vec4(cpu.velocity.x, cpu.velocity.y, cpu.angular_velocity, 0.0) * dt;

                    gpu.pos.w = cpu.initial_size * self.batched_size_curve.as_ref().map_or(1.0, |curve| curve.get(cpu.lived / cpu.lifetime));

                    if cpu.lifetime != 0.0 {
                        gpu.data.y = cpu.lived / cpu.lifetime;
                    }

                    cpu.lived += dt;
                    cpu.velocity += self.config.gravity * dt;

                    if let Some(atlas) = &self.config.atlas {
                        if cpu.lifetime != 0.0 {
                            cpu.frame = (cpu.lived / cpu.lifetime * (atlas.end_index - atlas.start_index) as f32) as u16 + atlas.start_index;
                        }

                        let x = cpu.frame % atlas.n;
                        let y = cpu.frame / atlas.n;

                        gpu.uv = vec4(x as f32 / atlas.n as f32, y as f32 / atlas.m as f32, 1.0 / atlas.n as f32, 1.0 / atlas.m as f32);
                    } else {
                        gpu.uv = vec4(0.0, 0.0, 1.0, 1.0);
                    }
                });
        }"""

assert old in c, 'pattern not found'
c = c.replace(old, new)
io.open(p, 'w', encoding='utf-8', newline='').write(c)
print('OK')
