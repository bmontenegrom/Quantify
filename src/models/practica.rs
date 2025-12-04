#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Practica {
    pub id: u32,
    pub nombre: &'static str,
    pub descripcion: &'static str,
}

pub fn practicas_iniciales() -> Vec<Practica> {
    vec![
        Practica { id: 1, nombre: "Práctica 1", descripcion: "Descripción de la práctica 1." },
        Practica { id: 2, nombre: "Práctica 2", descripcion: "Descripción de la práctica 2." },
        Practica { id: 3, nombre: "Práctica 3", descripcion: "Descripción de la práctica 3." },
        Practica { id: 4, nombre: "Práctica 4", descripcion: "Descripción de la práctica 4." },
        Practica { id: 5, nombre: "Práctica 5", descripcion: "Descripción de la práctica 5." },
        Practica { id: 6, nombre: "Práctica 6", descripcion: "Descripción de la práctica 6." },
        Practica { id: 7, nombre: "Práctica 7", descripcion: "Descripción de la práctica 7." },
        Practica { id: 8, nombre: "Práctica 8", descripcion: "Descripción de la práctica 8." },
        Practica { id: 9, nombre: "Práctica 9", descripcion: "Descripción de la práctica 9." },
        Practica { id: 10, nombre: "Práctica 10", descripcion: "Descripción de la práctica 10." },
    ]
}
