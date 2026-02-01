#version 100

precision highp float;

uniform float input_tf;
uniform float output_tf;
uniform mat3 input_to_output;

vec3 srgb_to_linear(vec3 color) {
    return pow(color, vec3(2.2));
}

vec3 linear_to_srgb(vec3 color) {
    return pow(color, vec3(1.0 / 2.2));
}

vec3 st2084_to_linear(vec3 color) {
    float m1 = 0.1593017578125;
    float m2 = 78.84375;
    float c1 = 0.8359375;
    float c2 = 18.8515625;
    float c3 = 18.6875;

    vec3 val = pow(color, vec3(1.0 / m2));
    return pow(max(val - c1, 0.0) / (c2 - c3 * val), vec3(1.0 / m1));
}

vec3 linear_to_st2084(vec3 color) {
    float m1 = 0.1593017578125;
    float m2 = 78.84375;
    float c1 = 0.8359375;
    float c2 = 18.8515625;
    float c3 = 18.6875;

    vec3 val = pow(color, vec3(m1));
    return pow((c1 + c2 * val) / (1.0 + c3 * val), vec3(m2));
}

vec3 power_to_linear(vec3 color) {
    return pow(color, vec3(input_tf - 10.0));
}

vec3 linear_to_power(vec3 color) {
    return pow(color, vec3(1.0 / (output_tf - 10.0)));
}

vec3 color_to_linear(vec3 color) {
    if (input_tf == 0.0) {
        return color;
    } else if (input_tf == 1.0) {
        return srgb_to_linear(color.rgb);
    } else if (input_tf == 2.0) {
        return st2084_to_linear(color.rgb);
    } else if (input_tf >= 10.0) {
        return power_to_linear(color.rgb);
    } else {
        return vec3(0.0);
    }
}

vec3 convert_linear_color(vec3 color_linear) {
    return input_to_output * color_linear;
}

vec3 linear_to_color(vec3 color_linear) {
    if (output_tf == 0.0) {
        return color_linear;
    } else if (output_tf == 1.0) {
        return linear_to_srgb(color_linear);
    } else if (output_tf == 2.0) {
        return linear_to_st2084(color_linear);
    } else if (output_tf >= 10.0) {
        return linear_to_power(color_linear);
    } else {
        return vec3(0.0);
    }
}

vec3 convert_color(vec3 color) {
    return linear_to_color(input_to_output * color_to_linear(color));
}
