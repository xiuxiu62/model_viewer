#version 330 core

in vec3 position;
layout(location = 3) in vec3 origin;

uniform mat4 shadowVP;

void main() {
    mat4 model_matrix = mat4(
        1.0, 0.0, 0.0, origin.x,
        0.0, 1.0, 0.0, origin.y,
        0.0, 0.0, 1.0, origin.z,
        0.0, 0.0, 0.0, 1.0
    );
    model_matrix = transpose(model_matrix);

    gl_Position = shadowVP * model_matrix * vec4(position, 1.0);
}