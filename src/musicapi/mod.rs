//
// mod.rs
// Copyright (C) 2019 gmg137 <gmg137@live.com>
// Distributed under terms of the GPLv3 license.
//
mod encrypt;
pub(crate) mod model;
use crate::model::{Errors, NCMResult, NCM_CONFIG};
use chrono::prelude::*;
use curl::easy::{Easy, List};
use encrypt::Encrypt;
use model::*;
use openssl::hash::{hash, MessageDigest};
use std::{collections::HashMap, fs, io::Read};

static BASE_URL: &str = "https://music.163.com";

pub struct MusicApi {
    curl: Easy,
}

impl MusicApi {
    #[allow(unused)]
    pub fn new() -> NCMResult<Self> {
        let mut headers = List::new();
        let mut curl = Easy::new();
        headers.append("Accept: */*")?;
        headers.append("Accept-Encoding: gzip,deflate,br")?;
        headers.append("Accept-Language: en-US,en;q=0.5")?;
        headers.append("Connection: keep-alive")?;
        headers.append("Content-Type: application/x-www-form-urlencoded")?;
        headers.append("Host: music.163.com")?;
        headers.append("Referer: https://music.163.com")?;
        headers.append("User-Agent: Mozilla/5.0 (X11; Linux x86_64; rv:65.0) Gecko/20100101 Firefox/65.0")?;
        curl.http_headers(headers)?;
        curl.accept_encoding("gzip")?;
        let cookie_path = format!("{}cookie", NCM_CONFIG.to_string_lossy());
        curl.cookie_file(cookie_path)?;
        curl.cookie_list("RELOAD")?;
        Ok(Self { curl })
    }

    // 发送请求
    // method: 请求方法
    // path: 请求路径
    // params: 请求参数
    // custom: 是否显示本机信息
    fn request(
        &mut self,
        method: Method,
        path: &str,
        params: &mut HashMap<String, String>,
        custom: bool,
    ) -> NCMResult<String> {
        let endpoint = format!("{}{}", BASE_URL, path);
        let mut csrf_token = String::new();
        if let Ok(cookies) = self.curl.cookies() {
            for cookie in cookies.iter() {
                let re = regex::Regex::new(r"__csrf\t(?P<csrf>\w+)$")?;
                let value = String::from_utf8_lossy(cookie);
                if let Some(caps) = re.captures(&value) {
                    if let Some(csrf) = caps.name("csrf") {
                        csrf_token = csrf.as_str().to_owned();
                    }
                    break;
                }
            }
        }
        let mut name = "";
        let mut value = "";
        if custom {
            name = "os";
            value = "pc"
        }
        self.curl.url(&endpoint)?;
        self.curl.timeout(std::time::Duration::from_secs(10))?;
        let mut contents = Vec::new();
        let local: DateTime<Local> = Local::now();
        let times = local.timestamp();
        let hextoken = hex::encode(hash(MessageDigest::md5(), &times.to_string().as_bytes())?);
        match method {
            Method::POST => {
                let make_cookie = format!("version=0;{}={};JSESSIONID-WYYY=%2FKSy%2B4xG6fYVld42G9E%2BxAj9OyjC0BYXENKxOIRH%5CR72cpy9aBjkohZ24BNkpjnBxlB6lzAG4D%5C%2FMNUZ7VUeRUeVPJKYu%2BKBnZJjEmqgpOx%2BU6VYmypKB%5CXb%2F3W7%2BDjOElCb8KlhDS2cRkxkTb9PBDXro41Oq7aBB6M6OStEK8E%2Flyc8%3A{}; _iuqxldmzr_=32; _ntes_nnid={},{}; _ntes_nuid={}", name, value,times,hextoken,hextoken,times+50);
                self.curl.cookie(&make_cookie)?;
                params.insert("csrf_token".to_owned(), csrf_token);
                let params = Encrypt::encrypt_request(params)?;
                self.curl.post(true)?;
                self.curl.post_field_size(params.len() as u64)?;
                let mut transfer = self.curl.transfer();
                transfer.read_function(|into| Ok(params.as_bytes().read(into).unwrap_or(0)))?;
                transfer.write_function(|data| {
                    contents.extend_from_slice(data);
                    Ok(data.len())
                })?;
                transfer.perform()?;
            }
            Method::GET => {
                self.curl.get(true)?;
                let mut transfer = self.curl.transfer();
                transfer.write_function(|data| {
                    contents.extend_from_slice(data);
                    Ok(data.len())
                })?;
                transfer.perform()?;
            }
        }
        if let Ok(cookies) = self.curl.cookies() {
            if !cookies.iter().collect::<Vec<&[u8]>>().is_empty() {
                let cookie_path = format!("{}cookie", NCM_CONFIG.to_string_lossy());
                self.curl.cookie_jar(cookie_path)?;
            }
        }
        Ok(String::from_utf8_lossy(&contents).to_string())
    }

    // 登录
    // username: 用户名(邮箱或手机)
    // password: 密码
    #[allow(unused)]
    pub fn login(&mut self, username: String, password: String) -> NCMResult<LoginInfo> {
        let mut params = HashMap::new();
        let path;
        if username.len().eq(&11) && username.parse::<u64>().is_ok() {
            path = "/weapi/login/cellphone";
            params.insert("phone".to_owned(), username);
            params.insert("password".to_owned(), password);
            params.insert("rememberLogin".to_owned(), "true".to_owned());
        } else {
            let client_token = "1_jVUMqWEPke0/1/Vu56xCmJpo5vP1grjn_SOVVDzOc78w8OKLVZ2JH7IfkjSXqgfmh";
            path = "/weapi/login";
            params.insert("username".to_owned(), username);
            params.insert("password".to_owned(), password);
            params.insert("rememberLogin".to_owned(), "true".to_owned());
            params.insert("clientToken".to_owned(), client_token.to_owned());
        }
        let result = self.request(Method::POST, path, &mut params, true)?;
        to_login_info(result)
    }

    // 登陆状态
    #[allow(unused)]
    pub fn login_status(&mut self) -> NCMResult<LoginInfo> {
        let result = self.request(Method::GET, "", &mut HashMap::new(), false)?;
        let re = regex::Regex::new(
            r#"userId:(?P<id>\d+),nickname:"(?P<nickname>\w+)",avatarUrl.+?(?P<avatar_url>http.+?jpg)""#,
        )?;
        let cap = re.captures(&result).ok_or(Errors::NoneError)?;
        let uid = cap.name("id").ok_or(Errors::NoneError)?.as_str().parse::<u64>()?;
        let nickname = cap.name("nickname").ok_or(Errors::NoneError)?.as_str().to_owned();
        let avatar_url = cap.name("avatar_url").ok_or(Errors::NoneError)?.as_str().to_owned();
        Ok(LoginInfo {
            code: 200,
            uid,
            nickname,
            avatar_url,
            msg: "已登录.".to_owned(),
        })
    }

    // 退出
    #[allow(unused)]
    pub fn logout(&mut self) {
        let cookie_path = format!("{}cookie", NCM_CONFIG.to_string_lossy());
        fs::write(&cookie_path, "").unwrap_or(());
    }

    // 每日签到
    #[allow(unused)]
    pub fn daily_task(&mut self) -> NCMResult<Msg> {
        let path = "/weapi/point/dailyTask";
        let mut params = HashMap::new();
        params.insert("type".to_owned(), "0".to_owned());
        let result = self.request(Method::POST, path, &mut params, false)?;
        to_msg(result)
    }

    // 用户歌单
    // uid: 用户id
    // offset: 列表起点号
    // limit: 列表长度
    #[allow(unused)]
    pub fn user_song_list(&mut self, uid: u64, offset: u8, limit: u8) -> NCMResult<Vec<SongList>> {
        let path = "/weapi/user/playlist";
        let mut params = HashMap::new();
        params.insert("uid".to_owned(), uid.to_string());
        params.insert("offset".to_owned(), offset.to_string());
        params.insert("limit".to_owned(), limit.to_string());
        params.insert("csrf_token".to_owned(), "".to_string());
        let result = self.request(Method::POST, path, &mut params, false)?;
        to_song_list(result, Parse::USL)
    }

    // 用户云盘
    #[allow(unused)]
    pub fn user_cloud_disk(&mut self) -> NCMResult<Vec<SongInfo>> {
        let path = "/weapi/v1/cloud/get";
        let mut params = HashMap::new();
        params.insert("offset".to_owned(), 0.to_string());
        params.insert("limit".to_owned(), 1000.to_string());
        params.insert("csrf_token".to_owned(), "".to_string());
        let result = self.request(Method::POST, path, &mut params, false)?;
        to_song_info(result, Parse::UCD)
    }

    // 歌单详情
    // songlist_id: 歌单 id
    #[allow(unused)]
    pub fn song_list_detail(&mut self, songlist_id: u64) -> NCMResult<Vec<SongInfo>> {
        let path = "/weapi/v3/playlist/detail";
        let mut params = HashMap::new();
        params.insert("id".to_owned(), songlist_id.to_string());
        params.insert("total".to_owned(), true.to_string());
        params.insert("limit".to_owned(), 1000.to_string());
        params.insert("offest".to_owned(), 0.to_string());
        params.insert("n".to_owned(), 1000.to_string());
        let result = self.request(Method::POST, path, &mut params, true)?;
        to_song_info(result, Parse::USL)
    }

    // 歌曲详情
    // ids: 歌曲 id 列表
    #[allow(unused)]
    pub fn songs_detail(&mut self, ids: &[u64]) -> NCMResult<Vec<SongInfo>> {
        let path = "/weapi/v3/song/detail";
        let mut params = HashMap::new();
        let mut json = String::from("[");
        for id in ids {
            let s = format!(r#"{{"id":{}}},"#, id);
            json.push_str(&s);
        }
        let mut json = json.trim_end_matches(",").to_owned();
        json.push_str("]");
        params.insert("c".to_owned(), json);
        params.insert("ids".to_owned(), serde_json::to_string(ids).unwrap_or("[]".to_owned()));
        let result = self.request(Method::POST, path, &mut params, false)?;
        to_song_info(result, Parse::USL)
    }

    // 歌曲 URL
    // ids: 歌曲列表
    // rate: 320: 320K,
    //       192: 192k
    //       128: 128k
    #[allow(unused)]
    pub fn songs_url(&mut self, ids: &[u64], rate: u32) -> NCMResult<Vec<SongUrl>> {
        let path = "/weapi/song/enhance/player/url";
        let mut params = HashMap::new();
        params.insert("ids".to_owned(), serde_json::to_string(ids)?);
        params.insert("br".to_owned(), (rate * 1000).to_string());
        let result = self.request(Method::POST, path, &mut params, false)?;
        to_song_url(result)
    }

    // 每日推荐歌单
    #[allow(unused)]
    pub fn recommend_resource(&mut self) -> NCMResult<Vec<SongList>> {
        let path = "/weapi/v1/discovery/recommend/resource";
        let mut params = HashMap::new();
        let result = self.request(Method::POST, path, &mut params, false)?;
        to_song_list(result, Parse::RMD)
    }

    // 每日推荐歌曲
    #[allow(unused)]
    pub fn recommend_songs(&mut self) -> NCMResult<Vec<SongInfo>> {
        let path = "/weapi/v2/discovery/recommend/songs";
        let mut params = HashMap::new();
        let result = self.request(Method::POST, path, &mut params, false)?;
        to_song_info(result, Parse::RMDS)
    }

    // 私人FM
    #[allow(unused)]
    pub fn personal_fm(&mut self) -> NCMResult<Vec<SongInfo>> {
        let path = "/weapi/v1/radio/get";
        let result = self.request(Method::POST, path, &mut HashMap::new(), false)?;
        to_song_info(result, Parse::RMD)
    }

    // 收藏/取消收藏
    // songid: 歌曲id
    // like: true 收藏，false 取消
    #[allow(unused)]
    pub fn like(&mut self, like: bool, songid: u64) -> bool {
        let path = "/weapi/radio/like";
        let mut params = HashMap::new();
        params.insert("alg".to_owned(), "itembased".to_owned());
        params.insert("trackId".to_owned(), songid.to_string());
        params.insert("like".to_owned(), like.to_string());
        params.insert("time".to_owned(), "25".to_owned());
        if let Ok(result) = self.request(Method::POST, path, &mut params, false) {
            return to_msg(result)
                .unwrap_or(Msg {
                    code: 0,
                    msg: "".to_owned(),
                })
                .code
                .eq(&200);
        }
        false
    }

    // FM 不喜欢
    // songid: 歌曲id
    #[allow(unused)]
    pub fn fm_trash(&mut self, songid: u64) -> bool {
        let path = "/weapi/radio/trash/add";
        let mut params = HashMap::new();
        params.insert("alg".to_owned(), "RT".to_owned());
        params.insert("songId".to_owned(), songid.to_string());
        params.insert("time".to_owned(), "25".to_owned());
        if let Ok(result) = self.request(Method::POST, path, &mut params, false) {
            return to_msg(result)
                .unwrap_or(Msg {
                    code: 0,
                    msg: "".to_owned(),
                })
                .code
                .eq(&200);
        }
        false
    }

    // 搜索
    // keywords: 关键词
    // types: 单曲(1)，歌手(100)，专辑(10)，歌单(1000)，用户(1002) *(type)*
    // offset: 起始点
    // limit: 数量
    #[allow(unused)]
    pub fn search(&mut self, keywords: String, types: u32, offset: u16, limit: u16) -> NCMResult<String> {
        let path = "/weapi/cloudsearch/get/web";
        let mut params = HashMap::new();
        params.insert("s".to_owned(), keywords);
        params.insert("type".to_owned(), types.to_string());
        params.insert("total".to_owned(), "true".to_string());
        params.insert("offset".to_owned(), offset.to_string());
        params.insert("limit".to_owned(), limit.to_string());
        let result = self.request(Method::POST, path, &mut params, false)?;
        match types {
            1 => to_song_info(result, Parse::SEARCH).and_then(|s| Ok(serde_json::to_string(&s)?)),
            100 => to_singer_info(result).and_then(|s| Ok(serde_json::to_string(&s)?)),
            _ => Err(Errors::NoneError),
        }
    }

    // 新碟上架
    // offset: 起始点
    // limit: 数量
    #[allow(unused)]
    pub fn new_albums(&mut self, offset: u8, limit: u8) -> NCMResult<Vec<SongList>> {
        let path = "/weapi/album/new";
        let mut params = HashMap::new();
        params.insert("area".to_owned(), "ALL".to_owned());
        params.insert("offset".to_owned(), offset.to_string());
        params.insert("limit".to_owned(), limit.to_string());
        params.insert("total".to_owned(), true.to_string());
        let result = self.request(Method::POST, path, &mut params, false)?;
        to_song_list(result, Parse::ALBUM)
    }

    // 专辑
    // album_id: 专辑 id
    #[allow(unused)]
    pub fn album(&mut self, album_id: u64) -> NCMResult<Vec<SongInfo>> {
        let path = format!("/weapi/v1/album/{}", album_id);
        let result = self.request(Method::POST, &path, &mut HashMap::new(), false)?;
        to_song_info(result, Parse::ALBUM)
    }

    // 热门推荐歌单
    // offset: 起始点
    // limit: 数量
    // order: 排序方式:
    //	      "hot": 热门，
    //        "new": 最新
    #[allow(unused)]
    pub fn top_song_list(&mut self, order: &str, offset: u8, limit: u8) -> NCMResult<Vec<SongList>> {
        let path = "/weapi/playlist/list";
        let mut params = HashMap::new();
        params.insert("cat".to_owned(), "全部".to_owned());
        params.insert("order".to_owned(), order.to_owned());
        params.insert("total".to_owned(), true.to_string());
        params.insert("offset".to_owned(), offset.to_string());
        params.insert("limit".to_owned(), limit.to_string());
        let result = self.request(Method::POST, path, &mut params, false)?;
        to_song_list(result, Parse::TOP)
    }

    // 热门歌曲/排行榜
    // list_id:
    // 云音乐飙升榜: 19723756
    // 云音乐新歌榜: 3779629
    // 网易原创歌曲榜: 2884035
    // 云音乐热歌榜: 3778678
    // 云音乐古典音乐榜: 71384707
    // 云音乐ACG音乐榜: 71385702
    // 云音乐韩语榜: 745956260
    // 云音乐国电榜: 10520166
    // 云音乐嘻哈榜: 991319590']
    // 抖音排行榜: 2250011882
    // UK排行榜周榜: 180106
    // 美国Billboard周榜: 60198
    // KTV嗨榜: 21845217
    // iTunes榜: 11641012
    // Hit FM Top榜: 120001
    // 日本Oricon周榜: 60131
    // 台湾Hito排行榜: 112463
    // 香港电台中文歌曲龙虎榜: 10169002
    // 华语金曲榜: 4395559
    #[allow(unused)]
    pub fn top_songs(&mut self, list_id: u64) -> NCMResult<Vec<SongInfo>> {
        self.song_list_detail(list_id)
    }

    // 查询歌词
    // music_id: 歌曲id
    #[allow(unused)]
    pub fn song_lyric(&mut self, music_id: u64) -> NCMResult<Vec<String>> {
        let path = "/weapi/song/lyric";
        let mut params = HashMap::new();
        params.insert("os".to_owned(), "osx".to_owned());
        params.insert("id".to_owned(), music_id.to_string());
        params.insert("lv".to_owned(), "-1".to_owned());
        params.insert("kv".to_owned(), "-1".to_owned());
        params.insert("tv".to_owned(), "-1".to_owned());
        let result = self.request(Method::POST, path, &mut params, false)?;
        to_lyric(result)
    }

    // 收藏/取消收藏歌单
    // like: true 收藏，false 取消
    // id: 歌单 id
    #[allow(unused)]
    pub fn song_list_like(&mut self, like: bool, id: u64) -> bool {
        let mut path = "/weapi/playlist/unsubscribe";
        if like {
            path = "/weapi/playlist/subscribe";
        }
        let mut params = HashMap::new();
        params.insert("id".to_owned(), id.to_string());
        if let Ok(result) = self.request(Method::POST, path, &mut params, false) {
            return to_msg(result)
                .unwrap_or(Msg {
                    code: 0,
                    msg: "".to_owned(),
                })
                .code
                .eq(&200);
        }
        false
    }
}
