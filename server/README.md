# (No longer used)
This code exists but is no longer used in the current version of mal-tui, as client id is now just added in the binary at compile time. And the oauth flow is handled directly in the client instead of through a relay server. However, the code is still available in the repository for reference.


# Server Part

This code runs on a server that handles user authentication and acts as a backend relay for mal-tui, keeping the client secret hidden from the client application.

## Why does this exist?

The server exists so users don't have to create their own MyAnimeList API credentials (Client ID and Client Secret) to use mal-tui. By default, mal-tui connects to my hosted server at `https://mal-tui.dogfetus.no`.

However, if you prefer to host the server yourself (either on your own server or locally), you can follow this guide. Note that you'll need to create your own API credentials on MyAnimeList.

## Self-Hosting Guide

### Step 1: Create MyAnimeList API Credentials

1. Go to https://myanimelist.net/apiconfig

2. Click **"Create ID"** under API Settings:
   ![Create ID](docs/myanimelist.png)

3. Fill in the required information:
   ![Fill information](docs/myanimelist2.png)
   
   **Important:** The **App Redirect URL** must match where your server is hosted plus `/callback`
   - Example: `http://localhost:8080/callback`
   - This must match the `MAL_REDIRECT_URL` in Step 2

4. Click **"Submit"** at the bottom of the page. Your app will be published and credentials will be generated:
   ![Published App](docs/myanimelist3.png)

5. Click into your app and note down the **Client ID** and **Client Secret**:
   ![Client Secret](docs/myanimelist4.png)

### Step 2: Configure Environment Variables

Create a `.env` file in your server directory with your credentials:

```env
MAL_CLIENT_ID=your_client_id_here
MAL_CLIENT_SECRET=your_client_secret_here
MAL_REDIRECT_URL=http://localhost:8080/callback
```

**Note:** Make sure `MAL_REDIRECT_URL` matches the redirect URL you set in Step 1.

### Step 3: Run the Server

**Recommended: Using Docker Compose**

Create a `compose.yaml` file in the same directory as your `.env`:

```yaml
version: "3.9"
services:
  app:
    image: dogfetus/mal-tui:latest 
    env_file:
      - .env
    ports:
      - "8000:8000"
    container_name: mal-tui
    restart: unless-stopped
networks:
  default:
    external: true
    name: public
```

Then run:
```bash
docker compose up -d
```

Verify it's running:
```bash
docker logs mal-tui
```

You should see: `"Now listening on localhost:8000"`

**Alternative: Build from Source**

You can also build and run the server from source if you prefer not to use Docker.

### Step 4: Configure mal-tui Client

Tell mal-tui to use your self-hosted server instead of the default one.

1. Generate and edit the config:
   ```bash
   mal -e
   ```

2. Change this line:
   ```toml
   [network]
   auth_server = "https://mal-tui.dogfetus.no"
   ```

   To:
   ```toml
   [network]
   auth_server = "http://localhost:8000"
   ```
   (Or whatever URL your server is hosted on)

That's it.
