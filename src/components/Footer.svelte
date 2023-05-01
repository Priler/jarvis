<script>
    import { invoke } from "@tauri-apps/api/tauri"

    import { tg_official_link, github_repository_link } from "@/stores";

    let current_year = new Date().getFullYear();
    let author_name = "";

    (async () => {
        author_name = await invoke("get_author_name")
        
    })().catch(err => {
        console.error(err);
    });
</script>

<footer id="footer">
    <p>© {current_year}. Автор проекта: {author_name}</p>
    <p style="margin-top: 5px;margin-bottom: 15px;">
        <a href="{tg_official_link}" target="_blank" class="special-link"><img src="/media/icons/howdy-logo.png" alt="" width="20px">&nbsp;&nbsp;Наш телеграм</a> канал.
        &nbsp;&nbsp;
        <a href="{github_repository_link}" target="_blank"><img src="/media/icons/github-logo.png" alt="" width="18px">&nbsp;Github репозиторий</a> проекта.</p>
</footer>

<style lang="scss">
    #footer {
        text-align: center;
        color: #565759;
        font-size: 12px;
        font-weight: bold;
        line-height: 1.7em;
        margin-top: 15px;

        p {
            margin: 0;
            padding: 0;
        }

        a {
            color: #185876;
            text-decoration: none;
            transition: opacity .5s;

            img {
                opacity: .5;
                transition: opacity .5s;
                margin-top: -4px;
            }

            &:hover {
                color: #2A9CD0;

                img {
                    opacity: 1;
                }
            }

            &.special-link {
                color: #941d92;
                display: inline-block;

                &:hover {
                    color: #FF07FC;

                    background: url(/media/images/bg/bg24.gif);
                    background-repeat: no-repeat;
                    background-size: contain;

                }
            }
        }
    }
</style>