use byteorder::BigEndian;
use nbtx::NbtError;
use serde::{Deserialize, Serialize};
use std::io::Cursor;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Dish {
    name: String,
    ingredients: Vec<Ingredient>,
    nutrition: NutritionFacts,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Ingredient {
    name: String,
    quantity: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct NutritionFacts {
    calories: i32,
    fat: f32,
    protein: f32,
    carbohydrates: f32,
}

fn main() {
    let rusty_burger = Dish {
        name: "Rusty Burger".to_string(),
        ingredients: vec![
            Ingredient {
                name: "Bun".to_string(),
                quantity: "1 piece".to_string(),
            },
            Ingredient {
                name: "Beef Patty".to_string(),
                quantity: "200g".to_string(),
            },
            Ingredient {
                name: "Cheese".to_string(),
                quantity: "2 slices".to_string(),
            },
        ],
        nutrition: NutritionFacts {
            calories: 800,
            fat: 50.0,
            protein: 40.0,
            carbohydrates: 60.0,
        },
    };

    let bytes = nbtx::to_bytes::<BigEndian>(&rusty_burger).unwrap();

    println!("{bytes:?}");

    let dish: Result<Dish, NbtError> =
        nbtx::from_bytes::<BigEndian, _>(&mut Cursor::new(bytes.as_slice()));
    match dish {
        Err(err) => println!("{err}"),
        Ok(dish) => println!("{dish:?}"),
    }

    //assert_eq!(dish, rusty_burger);
}
