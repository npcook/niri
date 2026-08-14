precision highp float;
#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

varying vec2 v_coords;

void main() {
    vec4 color = texture2D(tex, v_coords);

    gl_FragColor = clamp(convert_color_with_alpha(color), 0.0, 1.0);
}
