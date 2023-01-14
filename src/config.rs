use serde::{Deserialize, Serialize};
use crate::utils::get_accent_color;

#[derive(Deserialize,Serialize)]
pub struct Config{
    pub(crate) bg_color:[f32; 4],
    pub(crate) text_color:[f32; 4],
    pub(crate) font_size:i32,

}

impl Default for Config {
    fn default() -> Self {
        Config{
            bg_color: [0.0,0.0,0.0,0.5],
            text_color: [1.0,1.0,1.0,1.0],
            font_size: 24,
        }
    }
}

impl Config {
    pub fn check(&mut self){
        if self.bg_color[0] ==-1.0{
            get_accent_color().map(|color| {
                self.bg_color[0] = color.R as f32/255.0;
                self.bg_color[1] = color.G as f32/255.0;
                self.bg_color[2] = color.B as f32/255.0;
                self.bg_color[3] = color.A as f32/255.0;
                if self.bg_color[3]<=0.8 { self.bg_color[3] = 0.1; }else { self.bg_color[3] -= 0.8; }
            }).map_err(|e| {
               println!("get_accent_color error: {}", e);
                self.bg_color=[0.0,0.0,0.0,0.5];
            }).ok();
        }
        if self.text_color[0] ==-1.0{
            get_accent_color().map(|color| {
                self.text_color[0] = color.R as f32/255.0;
                self.text_color[1] = color.G as f32/255.0;
                self.text_color[2] = color.B as f32/255.0;
                self.text_color[3] = color.A as f32/255.0;
            }).map_err(|e| {
                println!("get_accent_color error: {}", e);
                self.text_color=[1.0,1.0,1.0,1.0];
            }).ok();
        }
         if self.bg_color.len() != 4||self.bg_color.map(|x| x<0.0||x>255.0).contains(&true){
            self.bg_color = [0.0,0.0,0.0,0.5];
        }
        self.bg_color=self.bg_color.map(|x| if x > 1.0 {x/255.0} else{x} );
        if self.text_color.len() != 4||self.text_color.map(|x| x<0.0||x>255.0).contains(&true){
            self.text_color = [1.0,1.0,1.0,1.0];
        }
        self.text_color=self.text_color.map(|x| if x > 1.0 {x/255.0} else{x} );
        if self.font_size < 0{
            self.font_size = 24;
        }

    }
}