use openvr::{Eye, System, RenderModels, TrackedDevicePose};
use rodio::Sink;
use crate::*;

pub fn openvr_to_mat4(mat: [[f32; 4]; 3]) -> glm::TMat4<f32> {
	glm::mat4(
		mat[0][0], mat[0][1], mat[0][2], mat[0][3],
		mat[1][0], mat[1][1], mat[1][2], mat[1][3],
		mat[2][0], mat[2][1], mat[2][2], mat[2][3],
		0.0, 0.0, 0.0, 1.0
	)
}

pub fn flatten_glm(mat: &glm::TMat4<f32>) -> [f32; 16] {
	let slice = glm::value_ptr(mat);

	let mut result = [0.0; 16];
	for i in 0..16 {
		result[i] = slice[i];
	}
	result
}

pub fn get_projection_matrix(sys: &System, eye: Eye) -> glm::TMat4<f32> {
	let t_matrix = sys.projection_matrix(eye, NEAR_Z, FAR_Z);
	glm::mat4(
		t_matrix[0][0], t_matrix[0][1], t_matrix[0][2], t_matrix[0][3],
		t_matrix[1][0], t_matrix[1][1], t_matrix[1][2], t_matrix[1][3],
		t_matrix[2][0], t_matrix[2][1], t_matrix[2][2], t_matrix[2][3],
		t_matrix[3][0], t_matrix[3][1], t_matrix[3][2], t_matrix[3][3]
	)
}

//This returns the Mesh struct associated with the OpenVR model
pub fn load_openvr_mesh(openvr_system: &Option<System>, openvr_rendermodels: &Option<RenderModels>, index: u32) -> Option<Mesh> {
	let mut result = None;
	if let (Some(ref sys), Some(ref ren_mod)) = (&openvr_system, &openvr_rendermodels) {
		let name = sys.string_tracked_device_property(index, openvr::property::RenderModelName_String).unwrap();
		if let Some(model) = ren_mod.load_render_model(&name).unwrap() {

			//Flatten each vertex into a simple &[f32]
			const ELEMENT_STRIDE: usize = 8;
			let mut vertices = Vec::with_capacity(ELEMENT_STRIDE * model.vertices().len());
			for vertex in model.vertices() {
				vertices.push(vertex.position[0]);
				vertices.push(vertex.position[1]);
				vertices.push(vertex.position[2]);
				vertices.push(vertex.normal[0]);
				vertices.push(vertex.normal[1]);
				vertices.push(vertex.normal[2]);
				vertices.push(vertex.texture_coord[0]);
				vertices.push(vertex.texture_coord[1]);
			}
			
			let mesh = unsafe {
				let vao = create_vertex_array_object(&vertices, model.indices(), &[3, 3, 2]);
				let tex_params = [
					(gl::TEXTURE_WRAP_S, gl::REPEAT),
					(gl::TEXTURE_WRAP_T, gl::REPEAT),
					(gl::TEXTURE_MIN_FILTER, gl::LINEAR),
					(gl::TEXTURE_MAG_FILTER, gl::LINEAR)
				];
				let image_data = ImageData {
					data: vec![128, 128, 128],
					width: 1,
					height: 1,
					format: gl::RGB,
					internal_format: gl::RGB
				};
				Mesh::new(vao, glm::identity(), load_texture_from_data(image_data, &tex_params), vec![0, model.indices().len() as GLsizei], None)
			};
			result = Some(mesh);
		}
	}
	result
}

pub fn get_frame_origin(something_to_world: &glm::TMat4<f32>) -> glm::TVec4<f32> {
	something_to_world * glm::vec4(0.0, 0.0, 0.0, 1.0)
}

pub fn get_mesh_origin(mesh: &Option<Mesh>) -> glm::TVec4<f32> {
	match mesh {
		Some(mesh) => {
			get_frame_origin(&mesh.model_matrix)
		}
		None => {
			println!("Couldn't return mesh origin cause it was \"None\"");
			glm::vec4(0.0, 0.0, 0.0, 1.0)
		}
	}
}

pub fn load_wavefront_obj(path: &str) -> Option<MeshData> {
	//Gracefully exit if this is not an obj
	if path.split_at(path.len() - 3).1 != "obj" {
		println!("{} is not an obj file, dude.", path);
		return None;
	}

	//Load .obj file's contents as a string
	let obj_contents = match fs::read_to_string(path) {
		Ok(s) => { s }
		Err(e) => {
			println!("{}", e);
			return None;
		}
	};

	//Load .mtl file's contents as a string
	let mtl_contents = match fs::read_to_string(format!("{}.mtl", path.split_at(path.len() - 4).0)) {
		Ok(s) => { Some(s) }
		Err(e) => {
			println!("{}", e);
			None
		}
	};
	
	//Parse the Objects from the file
	let obj_set = match obj::parse(obj_contents) {
		Ok(m) => { m }
		Err(e) => {
			println!("{:?}", e);
			return None;
		}
	};

	//Parse the Materials from the file
	let mtl_set = match mtl_contents {
		Some(contents) => {
			match mtl::parse(contents) {
				Ok(m) => { Some(m) }
				Err(e) => {
					println!("{:?}", e);
					None
				}
			}
		}
		None => None
	};

	//Transform the object into something the engine can actually use
	const BUFFER_SIZE: usize = 500;
	let mut index_map = HashMap::new();
	let mut vertices = Vec::with_capacity(BUFFER_SIZE);
	let mut indices = Vec::with_capacity(BUFFER_SIZE);
	let mut geometry_boundaries = Vec::with_capacity(BUFFER_SIZE);
	let mut materials_in_order = Vec::with_capacity(BUFFER_SIZE);
	let mut current_index = 0u16;
	let mut index_offset = 0;
	for object in obj_set.objects {
		for geo in &object.geometry {
			geometry_boundaries.push(indices.len() as GLsizei);

			//Copy the current material into materials_in_order
			match &mtl_set {
				Some(set) => {
					match &geo.material_name {
						Some(name) => {
							for material in &set.materials {
								if *name == material.name {
									materials_in_order.push(Some(material.clone()));
									break;
								}
							}
						}
						None => {
							materials_in_order.push(None);
						}
					}
				}
				None => materials_in_order.push(None)
			}

			for shape in &geo.shapes {
				match shape.primitive {
					obj::Primitive::Triangle(a, b, c) => {
						let verts = [a, b, c];
						for v in &verts {
							let pair = (v.0 + index_offset, v.2, v.1);
							match index_map.get(&pair) {
								Some(i) => {
									//This vertex has already been accounted for, so we can just push the index into indices
									indices.push(*i);
								}
								None => {
									//This vertex is not accounted for, and so now we must add its data to vertices

									//We add the position data to vertices
									vertices.push(object.vertices[pair.0 - index_offset].x as f32);
									vertices.push(object.vertices[pair.0 - index_offset].y as f32);
									vertices.push(object.vertices[pair.0 - index_offset].z as f32);

									//Push the normal vector data if there is any
									match pair.1 {
										Some(i) => {
											let coords = [object.normals[i].x as f32, object.normals[i].y as f32, object.normals[i].z as f32];
											for c in &coords {
												vertices.push(*c);
											}
										}
										None => {
											for _ in 0..3 {
												vertices.push(0.0);
											}
										}
									}

									//Push the texture coordinate data if there is any
									match pair.2 {
										Some(i) => {
											vertices.push(object.tex_vertices[i].u as f32);
											vertices.push(object.tex_vertices[i].v as f32);
										}
										None => {
											vertices.push(0.0);
											vertices.push(0.0);
										}
									}

									//Add the index to indices
									indices.push(current_index);

									//Then we declare that this vertex will appear in vertices at current_index
									index_map.insert(pair, current_index);
									current_index += 1;
								}
							}
						}
					}
					_ => {
						println!("load_wavefront_obj(): Only triangle meshes are supported.");
						return None;
					}
				}
			}
		}
		index_offset += current_index as usize;
	}
	geometry_boundaries.push(indices.len() as GLsizei);

	let vertex_array = VertexArray {
		vertices,
		indices,
		attribute_offsets: vec![3, 3, 2]
	};

	Some(MeshData {
		vertex_array,
		geo_boundaries: geometry_boundaries,
		materials: materials_in_order
	})
}

pub fn uniform_scale(scale: f32) -> glm::TMat4<f32> {
	glm::scaling(&glm::vec3(scale, scale, scale))
}

pub fn update_openvr_mesh(meshes: &mut OptionVec<Mesh>, poses: &[TrackedDevicePose], tracking_to_world: &glm::TMat4<f32>, device_index: usize, mesh_index: Option<usize>) {
	if let Some(mesh) = meshes.get_element(mesh_index) {
		mesh.model_matrix = tracking_to_world * openvr_to_mat4(*poses[device_index].device_to_absolute_tracking());
	}
}

pub fn halton_sequence(index: f32, base: f32) -> f32 {
	let mut f = 1.0;
	let mut r = 0.0;
	let mut i = index;

	while i > 0.0 {
		f = f / base;
		r = r + f * (i % base);
		i = f32::floor(i / base);
	}

	return r;
}

pub fn handle_result<T, E: std::fmt::Display>(result: Result<T, E>) {
	if let Err(e) = result {
		println!("{}", e);
	}
}

//Returns an array of n 4x4 matrices tightly packed in a Vec in column-major format
pub fn model_matrices_from_terrain(n: usize, halton_counter: &mut usize, terrain: &Terrain, scale: f32) -> Vec<f32> {
	//Allocate the buffer
	let mut model_matrices = vec![0.0; n * 16];

	//Populate the buffer
	for i in 0..n {
		let xpos = terrain.scale * (halton_sequence(*halton_counter as f32, 2.0) - 0.5);
		let zpos = terrain.scale * (halton_sequence(*halton_counter as f32, 3.0) - 0.5);
		*halton_counter += 1;
			
		//Height is simply the terrain height at position (x, z)
		let ypos = terrain.height_at(xpos, zpos);

		//Determine which floor triangle this tree is on
		let (moved_xpos, moved_zpos) = (xpos + (terrain.scale / 2.0), zpos + (terrain.scale / 2.0));
		let (subsquare_x, subsquare_z) = (f32::floor(moved_xpos * ((terrain.width - 1) as f32 / terrain.scale)) as usize,
										  f32::floor(moved_zpos * ((terrain.width - 1) as f32 / terrain.scale)) as usize);
		let subsquare_index = subsquare_x + subsquare_z * (terrain.width - 1);
		let (norm_x, norm_z) = (moved_xpos / (terrain.width - 1) as f32 + subsquare_x as f32 * terrain.scale / (terrain.width - 1) as f32,
								  moved_zpos / (terrain.width - 1) as f32 + subsquare_z as f32 * terrain.scale / (terrain.width - 1) as f32);
								  
		//Compute which triangle's normal to use
		let normal_index = if norm_x + norm_z <= 1.0 {
			subsquare_index * 2
		} else {
			subsquare_index * 2 + 1
		};

		let rotation_vector = glm::cross::<f32, glm::U3>(&glm::vec3(0.0, 1.0, 0.0), &terrain.surface_normals[normal_index]);
		let rotation_magnitude = 0.2 * f32::acos(glm::dot(&glm::vec3(0.0, 1.0, 0.0), &terrain.surface_normals[normal_index])); //Note: Multiplying rotation angle by 0.2 because that looks good enough and I can't tell how my math is wrong

		//Contruct the matrix itself
		let matrix = glm::translation(&glm::vec3(xpos, ypos, zpos)) *														//4: Translate the object to its proper position
					 glm::rotation(rotation_magnitude, &rotation_vector) *													//3: Rotate the object so its flush(ish) with the terrain
					 glm::rotation(rand::random::<f32>() * glm::half_pi::<f32>(), &glm::vec3(0.0, 1.0, 0.0)) *				//2: Rotate the object a random amount around it's y-axis
					 uniform_scale(scale);																					//1: Scale the object

		//Write this matrix to the buffer
		for j in 0..16 {
			model_matrices[i * 16 + j] = matrix[j];
		}
	}

	model_matrices
}

pub fn add_source_from_file(sink: &Sink, bgm_path: &str) {
	let source = rodio::Decoder::new(BufReader::new(File::open(bgm_path).unwrap())).unwrap();
	sink.append(source);
}

pub fn play_bgm(device: &rodio::Device, filename: &str, volume: f32) -> rodio::Sink {	
	let sink = rodio::Sink::new(device);
	add_source_from_file(&sink, &filename);
	sink.set_volume(volume);
	sink.play();
	sink
}

pub unsafe fn instanced_prop_vao(vertex_array: &VertexArray, terrain: &Terrain, instances: usize, halton_counter: &mut usize, scale: f32) -> GLuint {
	let vao = create_vertex_array_object(&vertex_array.vertices, &vertex_array.indices, &vertex_array.attribute_offsets);
	let model_matrices = model_matrices_from_terrain(instances, halton_counter, &terrain, scale);
	bind_instanced_matrices(vao, vertex_array.attribute_offsets.len(), &model_matrices, instances);
	vao
}

//Maps x from [0, window_width] to [-1.0, 1.0] and maps y from [0, window_height] to [1.0, -1.0]
pub fn pixel_matrix(window_size: (u32, u32)) -> glm::TMat4<f32> {
	glm::mat4(
		2.0 / window_size.0 as f32, 0.0, 0.0, -1.0,
		0.0, -2.0 / window_size.1 as f32, 0.0, 1.0,
		0.0, 0.0, 1.0, 0.0,
		0.0, 0.0, 0.0, 1.0
	)
}

//Invoke file selection dialogue, and return the resulting filepath as a String
pub fn file_select() -> Option<String> {
	let mut dialog = nfd::open_file_dialog(None, None);
	while let Err(_) = dialog {
		dialog = nfd::open_file_dialog(None, None);
	}
	match dialog.unwrap() {
		Response::Okay(filename) => { Some(filename) }
		_ => { None }
	}
}