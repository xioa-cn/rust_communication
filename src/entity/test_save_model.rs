pub struct SaveModel {
    id: u16,
    name: String,
    data: String,
}

impl SaveModel {
    pub fn new(id: u16, name: String, data: String) -> SaveModel {
        SaveModel { id, name, data }
    }

    pub fn from(id: u16, name: &str, data: &str) -> SaveModel {
        let name = String::from(name);
        let data = String::from(data);
        SaveModel { id, name, data }
    }

    pub fn get_id(&self) -> u16 {
        self.id
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_data(&self) -> &str {
        &self.data
    }

    pub fn set_data(&mut self, data: &str) {
        self.data = String::from(data) ;
    }

    pub fn set_id(&mut self, id: u16) {
        self.id = id;
    }

    pub fn set_name(&mut self, name:  &str) {
        self.name = String::from(name) ;
    }
}
