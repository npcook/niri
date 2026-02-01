precision highp float;
#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

varying vec2 v_coords;

void main() {
    vec4 color = texture2D(tex, v_coords);

    vec3 color_output = convert_color(color.rgb);

    gl_FragColor = vec4(clamp(color_output, 0.0, 1.0), color.a);
}
