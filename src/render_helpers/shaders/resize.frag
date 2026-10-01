vec4 color_to_linear_with_alpha(vec4 color);
vec4 linear_to_color_with_alpha(vec4 color);

vec4 resize_color(vec3 coords_curr_geo, vec3 size_curr_geo) {
    vec3 coords_tex_prev = niri_geo_to_tex_prev * coords_curr_geo;
    vec4 color_prev = color_to_linear_with_alpha(texture2D(niri_tex_prev, coords_tex_prev.st));

    vec3 coords_tex_next = niri_geo_to_tex_next * coords_curr_geo;
    vec4 color_next = color_to_linear_with_alpha(texture2D(niri_tex_next, coords_tex_next.st));

    vec4 color = mix(color_prev, color_next, niri_clamped_progress);
    return linear_to_color_with_alpha(color);
}
