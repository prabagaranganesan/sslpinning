# Spring Boot leaves both the runnable fat JAR and *.jar.original in target/.
# Do not use COPY ... *.jar app.jar — multiple matches make the build fail.
FROM maven:3.9.9-eclipse-temurin-17 AS build
ARG RENDER_GIT_COMMIT=unknown
WORKDIR /app
COPY pom.xml .
COPY src ./src
ENV MAVEN_OPTS="-Xmx512m"
RUN mvn -B -DskipTests package \
    && JAR=$(ls target/*-SNAPSHOT.jar 2>/dev/null | grep -v '\.jar\.original$' | head -1) \
    && cp "$JAR" /app/application.jar

FROM eclipse-temurin:17-jre-jammy
ARG RENDER_GIT_COMMIT=unknown
WORKDIR /app
RUN groupadd --system app && useradd --system --gid app app
COPY --from=build /app/application.jar app.jar
USER app
ENV PORT=8080
ENV APP_GIT_SHA=${RENDER_GIT_COMMIT}
EXPOSE 8080
ENTRYPOINT ["sh", "-c", "exec java -jar app.jar"]
