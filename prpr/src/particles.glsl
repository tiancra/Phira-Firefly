#ifdef DEF_VERTEX_ATTRIBUTES
attribute vec3 in_attr_pos;
attribute vec2 in_attr_uv;
attribute vec4 in_attr_color;
attribute vec4 in_attr_inst_pos;
attribute vec4 in_attr_inst_uv;
attribute vec4 in_attr_inst_data;
attribute vec4 in_attr_inst_color;
uniform mat4 _mvp;
uniform float _local_coords;
uniform vec3 _emitter_position;
uniform float _time;
uniform float _gpu_driven;
uniform vec4 _color_start;
uniform vec4 _color_mid;
uniform vec4 _color_end;
uniform float _atlas_n;
uniform float _atlas_m;
uniform float _atlas_start;
uniform float _atlas_end;
uniform vec2 _gravity;
uniform float _linear_accel;

lowp mat2 rotate2d(float angle){
    return mat2(cos(angle),-sin(angle),
                sin(angle),cos(angle));
}
vec4 particle_transform_vertex() {
     vec4 transformed = vec4(0.0, 0.0, 0.0, 0.0);
     mat2 rot = rotate2d(in_attr_inst_pos.z);
     vec4 in_attr_inst_pos = vec4(in_attr_inst_pos.xy, 0.0, in_attr_inst_pos.w);
     if (_local_coords == 0.0) {
        transformed = vec4(vec3(rot * in_attr_pos.xy, in_attr_pos.z) * in_attr_inst_pos.w + in_attr_inst_pos.xyz, 1.0);
     } else {
        transformed = vec4(vec3(rot * in_attr_pos.xy, in_attr_pos.z) * in_attr_inst_pos.w + in_attr_inst_pos.xyz +
                        _emitter_position.xyz, 1.0);
     }
     return _mvp * transformed;
}

vec2 particle_transform_uv() {
    return in_attr_uv * in_attr_inst_uv.zw + in_attr_inst_uv.xy;
}

// GPU-driven particle: position/color/UV computed from birth_time in shader
// in_attr_inst_pos: (initial_x, initial_y, rotation, initial_size)
// in_attr_inst_data: (birth_time, lifetime, velocity_x, velocity_y)
// in_attr_inst_color: base_color (rgba)
vec4 particle_transform_vertex_gpu(out lowp vec4 out_color, out lowp vec2 out_uv) {
    float birth = in_attr_inst_data.x;
    float lifetime = in_attr_inst_data.y;
    float age = _time - birth;

    if (age >= lifetime || lifetime <= 0.0) {
        out_color = vec4(0.0);
        out_uv = vec2(0.0);
        return vec4(0.0, 0.0, -1000.0, 1.0);
    }

    float t = age / lifetime;
    vec2 vel = in_attr_inst_data.zw;

    // position: initial + vel*age + 0.5*gravity*age^2 + 0.5*linear_accel*age^2 (along vel)
    vec2 pos = in_attr_inst_pos.xy + vel * age + 0.5 * _gravity * age * age;
    if (length(vel) > 0.001) {
        pos += normalize(vel) * _linear_accel * 0.5 * age * age;
    }

    float rotation = in_attr_inst_pos.z;
    float size = in_attr_inst_pos.w;

    // color curve (start -> mid -> end)
    vec4 col;
    if (t < 0.5) {
        col = mix(_color_start, _color_mid, t * 2.0);
    } else {
        col = mix(_color_mid, _color_end, (t - 0.5) * 2.0);
    }
    out_color = col * in_attr_inst_color;

    // atlas UV
    float frame_count = _atlas_end - _atlas_start;
    float frame = floor(t * frame_count) + _atlas_start;
    if (frame >= _atlas_end) frame = _atlas_end - 1.0;
    float fx = mod(frame, _atlas_n);
    float fy = floor(frame / _atlas_n);
    out_uv = in_attr_uv * vec2(1.0 / _atlas_n, 1.0 / _atlas_m) + vec2(fx / _atlas_n, fy / _atlas_m);

    mat2 rot = rotate2d(rotation);
    vec4 transformed;
    if (_local_coords == 0.0) {
        transformed = vec4(vec3(rot * in_attr_pos.xy, in_attr_pos.z) * size + vec3(pos, 0.0), 1.0);
    } else {
        transformed = vec4(vec3(rot * in_attr_pos.xy, in_attr_pos.z) * size + vec3(pos, 0.0) + _emitter_position.xyz, 1.0);
    }
    return _mvp * transformed;
}
#endif

highp float rand(lowp vec2 co) {
    highp float a = 12.9898;
    highp float b = 78.233;
    highp float c = 43758.5453;
    highp float dt= dot(co.xy ,vec2(a,b));
    highp float sn= mod(dt,3.14);
    return fract(sin(sn) * c);
}

lowp float particle_ix(lowp vec4 particle_data) {
    return particle_data.x;
}

lowp float particle_lifetime(lowp vec4 particle_data) {
    return particle_data.y;
}
